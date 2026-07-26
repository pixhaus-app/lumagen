//! The generation orchestrator: turns "generate the material" into a concurrent batch of
//! provider calls for the **semantic** maps (roughness, metallic, emission — and albedo/height
//! when AI), with the **geometric** maps (normal, AO, opacity, derived height) computed
//! deterministically by [`crate::derive`]. Results stream back over a channel the UI drains.
//!
//! Network-bound, so the ~3 semantic calls run concurrently (`buffer_unordered` bounded by a
//! `Semaphore` sized to `max_concurrency`), each cancelable via a child `CancellationToken`,
//! with retry/backoff on idempotent failures only. The mock provider runs this whole path
//! with zero network.

use std::sync::Arc;

use futures::stream::{self, StreamExt as _};
use tokio::sync::{Semaphore, mpsc};

use crate::data::{self, MapId};
use crate::derive::{self, DeriveParams, DerivePath};
use crate::providers::{BackendParams, ImageProvider, ImageRequest, ImageSpec, RequestKind};

/// How each map is produced for a given run (AI ⇄ derived per map).
#[derive(Clone, Debug)]
pub struct GenerationPlan {
    /// The derivation path per map.
    pub paths: [DerivePath; 8],
    /// Provider model slug (fal model id or OpenRouter slug) from settings.
    pub model: String,
    /// The fal/openrouter backend params to attach to each request.
    pub backend: BackendParams,
    /// Max concurrent provider calls (Settings → Generation).
    pub max_concurrency: usize,
    /// Retry attempts on idempotent failures.
    pub retries: usize,
    /// Requested output size (applied to text-to-image; edits inherit the reference size).
    pub size: Option<ImageSpec>,
    /// Per-map derivation parameters (the non-destructive adjust sliders), indexed by
    /// `MapId::index()`. Used for every `DerivePath::Derived` map so the geometric maps honor
    /// the user's strength/invert/flip_y/radius/detail_routing.
    pub derive_params: [DeriveParams; 8],
}

impl Default for GenerationPlan {
    fn default() -> Self {
        Self {
            paths: std::array::from_fn(|i| derive::default_path(MapId::ALL[i])),
            model: "fal-ai/flux-pro/kontext".into(),
            backend: BackendParams::Fal {
                guidance_scale: Some(3.5),
                num_inference_steps: None,
                strength: None,
                negative_prompt: None,
                sync_mode: Some(false),
                quality: None,
                extra: serde_json::Value::Null,
            },
            max_concurrency: 3,
            retries: 3,
            size: Some(ImageSpec::Aspect("1:1".into())),
            derive_params: [DeriveParams::default(); 8],
        }
    }
}

/// Inputs needed to compile the prompt for a map.
#[derive(Clone, Debug)]
pub struct PromptContext {
    /// Base/system template.
    pub base_template: String,
    /// Final-requirement suffix.
    pub final_requirement: String,
    /// The material's semantic description.
    pub description: String,
    /// Per-map instruction templates, indexed by `MapId::index()`. An empty entry falls back to
    /// the built-in default for that map. This is what carries the user's Settings → Prompts
    /// per-map edits into the request.
    pub map_templates: [String; 8],
    /// Negative prompt, injected into the backend params of every request.
    pub negative: String,
}

impl Default for PromptContext {
    fn default() -> Self {
        Self {
            base_template: String::new(),
            final_requirement: String::new(),
            description: String::new(),
            map_templates: std::array::from_fn(|_| String::new()),
            negative: String::new(),
        }
    }
}

/// A full-resolution square master: RGBA8 pixels + side length.
pub type MasterPixels = (Vec<u8>, usize);

/// The result of one finished map, sent back to the UI.
#[derive(Debug)]
pub enum MapOutcome {
    /// A map's pixels are ready (RGBA8 at the working size).
    Ready {
        /// Which map.
        map: MapId,
        /// RGBA8 pixels at the working size (drives the UI).
        rgba: Vec<u8>,
        /// The model's full-resolution square output `(rgba, side)`, kept for export quality.
        /// `None` when the source was already at (or below) the working size.
        master: Option<MasterPixels>,
        /// Real cost where the provider reports it (OpenRouter); `None` otherwise.
        cost: Option<f64>,
    },
    /// A map failed.
    Failed {
        /// Which map.
        map: MapId,
        /// Why.
        error: String,
    },
    /// A set of variation candidates for one map (one provider call, N images).
    Variations {
        /// Which map.
        map: MapId,
        /// The candidate RGBA8 images at the working size (one per requested variation).
        candidates: Vec<Vec<u8>>,
        /// Each candidate's full-resolution master, aligned with `candidates`.
        masters: Vec<Option<MasterPixels>>,
        /// Real cost where the provider reports it.
        cost: Option<f64>,
    },
}

/// The orchestrator owns the provider and runs generation batches.
pub struct MapGenerator {
    provider: Arc<dyn ImageProvider>,
}

impl MapGenerator {
    /// Wrap a provider.
    pub fn new(provider: Arc<dyn ImageProvider>) -> Self {
        Self { provider }
    }

    /// Compile the prompt for a semantic map (base + material semantics + map template + final),
    /// substituting the `{seed}`/`{resolution}` tokens with their runtime values. Uses the user's
    /// per-map template edit when present, else the built-in default.
    fn compile(ctx: &PromptContext, map: MapId, seed: i64, resolution: usize) -> String {
        let user = &ctx.map_templates[map.index()];
        let tpl = if user.trim().is_empty() {
            data::default_map_template(map)
        } else {
            user.as_str()
        };
        data::compile_prompt_with(
            &ctx.base_template,
            &ctx.final_requirement,
            &ctx.description,
            tpl,
            seed as u32,
            &resolution.to_string(),
        )
    }

    /// Attach the user's negative prompt to the backend params of a request. fal takes a typed
    /// `negative_prompt` field; OpenRouter carries it inside `image_config`. A non-empty backend
    /// value already set (e.g. from settings) wins over the context negative.
    fn with_negative(backend: &BackendParams, negative: &str) -> BackendParams {
        let neg = negative.trim();
        match backend {
            BackendParams::Fal {
                guidance_scale,
                num_inference_steps,
                strength,
                negative_prompt,
                sync_mode,
                quality,
                extra,
            } => BackendParams::Fal {
                guidance_scale: *guidance_scale,
                num_inference_steps: *num_inference_steps,
                strength: *strength,
                negative_prompt: negative_prompt.clone().or_else(|| (!neg.is_empty()).then(|| neg.to_string())),
                sync_mode: *sync_mode,
                quality: quality.clone(),
                extra: extra.clone(),
            },
            BackendParams::OpenRouter {
                modalities,
                image_config,
                provider,
            } => {
                let mut cfg = image_config.clone();
                if !neg.is_empty()
                    && let Some(o) = cfg.as_object_mut()
                {
                    o.entry("negative_prompt".to_string()).or_insert_with(|| neg.into());
                }
                BackendParams::OpenRouter {
                    modalities: modalities.clone(),
                    image_config: cfg,
                    provider: provider.clone(),
                }
            }
        }
    }

    /// Generate the requested subset of maps: the semantic (AI) ones concurrently via the
    /// provider, the derived (geometric) ones computed from the albedo. Only maps in `targets`
    /// are (re)produced — so a single-map re-roll or "regenerate missing" touches just those,
    /// never the whole set. Streams each finished map over `tx`.
    ///
    /// `albedo` (working size) drives the geometric derivations; `albedo_master` is the
    /// full-resolution square albedo `(rgba, side)` when one exists — the provider reference
    /// is a REAL PNG encoded from the highest-resolution source available, so edits see the
    /// full detail (and receive an actual image, not raw RGBA labeled as PNG).
    #[tracing::instrument(skip_all, fields(model = %plan.model, n = targets.len()))]
    pub async fn generate_material(
        &self,
        albedo: Vec<u8>,
        albedo_master: Option<MasterPixels>,
        size: usize,
        targets: Vec<MapId>,
        prompt: PromptContext,
        plan: GenerationPlan,
        seed: i64,
        tokens: std::collections::HashMap<MapId, tokio_util::sync::CancellationToken>,
        tx: mpsc::UnboundedSender<MapOutcome>,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        let target: std::collections::HashSet<MapId> = targets.into_iter().collect();

        // ── 1. Derived (geometric) maps in the target set (instant, offline, aligned) ──
        // Each derived map honors its own per-map adjust params (strength/invert/flip_y/radius/
        // detail_routing), computing its own height from the albedo — matching the export path
        // exactly so the preview and the exported file agree. They run FIRST so the zero-latency
        // maps never queue behind minutes of network traffic, and the batch token is checked
        // between steps so a cancelled batch stops emitting outcomes instead of resurrecting
        // maps after the user pressed Stop.
        for id in MapId::DERIVED {
            if cancel.is_cancelled() {
                return;
            }
            if target.contains(&id) && plan.paths[id.index()] == DerivePath::Derived {
                let params = plan.derive_params[id.index()];
                let rgba = derive::derive_map(id, &albedo, None, size, &params);
                let _ = tx.send(MapOutcome::Ready {
                    map: id,
                    rgba,
                    master: None,
                    cost: Some(0.0),
                });
            }
        }

        // ── 2. Semantic (AI) maps in the target set, concurrently via the provider ──
        let semantic: Vec<MapId> = MapId::DERIVED
            .iter()
            .copied()
            .filter(|id| target.contains(id) && plan.paths[id.index()] == DerivePath::Ai)
            .collect();

        let ref_side = albedo_master.as_ref().map(|m| m.1).unwrap_or(size);
        let edit_size = edit_size_spec(ref_side, plan.size.as_ref());

        // Encode the reference once, shared across map tasks.
        let reference_png: Option<Arc<Vec<u8>>> = if semantic.is_empty() {
            None
        } else {
            encode_reference_png(&albedo, size, albedo_master).await.map(Arc::new)
        };

        let sem = Semaphore::new(plan.max_concurrency.max(1));
        let mut tasks = stream::iter(semantic.into_iter().map(|map| {
            let provider = Arc::clone(&self.provider);
            let sem = &sem;
            let prompt = prompt.clone();
            let plan = plan.clone();
            let reference = reference_png.clone();
            let edit_size = edit_size.clone();
            let tx = tx.clone();
            // Use the caller-provided per-map token (so the UI can cancel this one map), falling
            // back to a child of the batch token.
            let cancel = tokens.get(&map).cloned().unwrap_or_else(|| cancel.child_token());
            async move {
                let _permit = sem.acquire().await;
                let references = match &reference {
                    Some(png) => vec![crate::providers::ImageInput::Bytes {
                        data: (**png).clone(),
                        mime: "image/png",
                    }],
                    None => Vec::new(),
                };
                let request = ImageRequest {
                    kind: RequestKind::ImageEdit,
                    map,
                    model: plan.model.clone(),
                    prompt: Self::compile(&prompt, map, seed, size),
                    references,
                    size: edit_size.clone(),
                    seed: Some(seed),
                    num_images: Some(1),
                    output_format: Some(crate::providers::ImageFormat::Png),
                    backend: Self::with_negative(&plan.backend, &prompt.negative),
                };
                let result = Self::generate_with_retry(&*provider, &request, plan.retries, cancel, SquareMode::Stretch).await;
                match result {
                    Ok((rgba, master, cost)) => {
                        let _ = tx.send(MapOutcome::Ready { map, rgba, master, cost });
                    }
                    Err(e) => {
                        let _ = tx.send(MapOutcome::Failed { map, error: e.to_string() });
                    }
                }
            }
        }))
        .buffer_unordered(plan.max_concurrency.max(1));

        while let Some(()) = tasks.next().await {}
    }

    /// Generate `count` variation candidates for one map in a single provider call
    /// (`num_images`), streaming the decoded candidates back as one `Variations` outcome. The
    /// mock provider synthesizes `count` distinct candidates offline; real providers return
    /// however many images the backend produced (the UI promotes whichever the user picks).
    #[tracing::instrument(skip_all, fields(?map, model = %plan.model))]
    pub async fn generate_variations(
        &self,
        albedo: Vec<u8>,
        albedo_master: Option<MasterPixels>,
        map: MapId,
        count: usize,
        prompt: PromptContext,
        plan: GenerationPlan,
        seed: i64,
        tx: mpsc::UnboundedSender<MapOutcome>,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        let ref_side = albedo_master.as_ref().map(|m| m.1).unwrap_or(crate::maps::TEX_SIZE);
        let reference_png = encode_reference_png(&albedo, crate::maps::TEX_SIZE, albedo_master).await;
        let references = match reference_png {
            Some(png) => vec![crate::providers::ImageInput::Bytes { data: png, mime: "image/png" }],
            None => Vec::new(),
        };
        let request = ImageRequest {
            kind: RequestKind::ImageEdit,
            map,
            model: plan.model.clone(),
            prompt: Self::compile(&prompt, map, seed, crate::maps::TEX_SIZE),
            references,
            size: edit_size_spec(ref_side, plan.size.as_ref()),
            seed: Some(seed),
            num_images: Some(count as u32),
            output_format: Some(crate::providers::ImageFormat::Png),
            backend: Self::with_negative(&plan.backend, &prompt.negative),
        };
        match Self::generate_multi_with_retry(&*self.provider, &request, plan.retries, cancel, SquareMode::Stretch).await {
            Ok((candidates, masters, cost)) => {
                let _ = tx.send(MapOutcome::Variations {
                    map,
                    candidates,
                    masters,
                    cost,
                });
            }
            Err(e) => {
                let _ = tx.send(MapOutcome::Failed { map, error: e.to_string() });
            }
        }
    }

    /// Generate a new albedo from a text prompt (the "Generate from prompt" wizard). A
    /// text-to-image call with no reference image; the mock provider renders a procedural albedo
    /// offline. Streams the result back as a `Ready { Albedo }` outcome.
    #[tracing::instrument(skip(self, plan, tx, cancel), fields(model = %plan.model))]
    pub async fn generate_albedo(
        &self,
        description: String,
        negative: String,
        plan: GenerationPlan,
        seed: i64,
        tx: mpsc::UnboundedSender<MapOutcome>,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        // Note: no pixel size in the prompt — the working size is an app detail; the request's
        // size spec (1:1 square) tells the backend what to produce.
        let prompt = format!(
            "{description}\n\nSeamless, tileable PBR ALBEDO / base-color texture. Flat orthographic top-down view, even flat lighting, no shadows, highlights, reflections, or baked lighting. Square 1:1, edge-to-edge seamless."
        );
        let request = ImageRequest {
            kind: RequestKind::TextToImage,
            map: MapId::Albedo,
            model: plan.model.clone(),
            prompt,
            references: Vec::new(),
            size: plan.size.clone(),
            seed: Some(seed),
            num_images: Some(1),
            output_format: Some(crate::providers::ImageFormat::Png),
            backend: Self::with_negative(&plan.backend, &negative),
        };
        match Self::generate_with_retry(&*self.provider, &request, plan.retries, cancel, SquareMode::Crop).await {
            Ok((rgba, master, cost)) => {
                let _ = tx.send(MapOutcome::Ready {
                    map: MapId::Albedo,
                    rgba,
                    master,
                    cost,
                });
            }
            Err(e) => {
                let _ = tx.send(MapOutcome::Failed {
                    map: MapId::Albedo,
                    error: e.to_string(),
                });
            }
        }
    }

    /// Upscale one map's pixels via the provider's upscaler (Topaz on fal; a plain resize on
    /// the offline mock), streaming the result back as a `Ready` outcome — a new
    /// full-resolution master plus a refreshed working-size preview. Targets beyond the
    /// upscaler's 4× factor cap are reached by chaining passes (each billed separately).
    #[tracing::instrument(skip_all, fields(?map, model = %model, side, target_side))]
    pub async fn upscale_map(
        &self,
        map: MapId,
        rgba: Vec<u8>,
        side: usize,
        target_side: usize,
        model: String,
        retries: usize,
        tx: mpsc::UnboundedSender<MapOutcome>,
        cancel: tokio_util::sync::CancellationToken,
    ) {
        let chain = upscale_chain(side, target_side);
        let mut src = rgba;
        let mut src_side = side;
        let mut total_cost = 0.0f64;
        let mut last_working: Option<Vec<u8>> = None;
        let mut failure: Option<crate::providers::ProviderError> = None;
        for (pass, next) in chain.iter().enumerate() {
            tracing::info!(?map, pass, from = src_side, to = next, "upscale pass");
            // `src` is dead after this call (the pass output replaces it), so hand the
            // buffer over instead of cloning it — at the 16K target a clone is a gigabyte.
            match self
                .upscale_once(map, std::mem::take(&mut src), src_side, *next, model.clone(), retries, &cancel)
                .await
            {
                Ok((working, master, cost)) => {
                    total_cost += cost.unwrap_or(0.0);
                    // The next pass reads the highest-resolution buffer available; the final
                    // outcome's master is rebuilt from (src, src_side) after the loop.
                    if let Some((mpx, mside)) = master {
                        src = mpx;
                        src_side = mside;
                    } else {
                        src.clone_from(&working);
                        src_side = crate::maps::TEX_SIZE;
                    }
                    last_working = Some(working);
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        let outcome = match (failure, last_working) {
            (None, Some(working)) => MapOutcome::Ready {
                map,
                rgba: working,
                master: (src_side > crate::maps::TEX_SIZE).then_some((src, src_side)),
                cost: Some(total_cost),
            },
            (Some(e), _) => MapOutcome::Failed {
                map,
                error: format!("upscale: {e}"),
            },
            // An empty chain (source already at/above the target) produces no passes.
            (None, None) => MapOutcome::Failed {
                map,
                error: format!("upscale: {}", crate::providers::ProviderError::Empty),
            },
        };
        let _ = tx.send(outcome);
    }

    /// One upscaler pass with retry/backoff. Returns `(working, master, cost)`.
    async fn upscale_once(
        &self,
        map: MapId,
        rgba: Vec<u8>,
        side: usize,
        target_side: usize,
        model: String,
        retries: usize,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Result<(Vec<u8>, Option<MasterPixels>, Option<f64>), crate::providers::ProviderError> {
        let req = crate::providers::UpscaleRequest {
            map,
            rgba,
            side,
            target_side,
            model,
        };
        let progress: &crate::providers::ProgressFn = &|_p| {};
        let resp = call_with_retry(retries, cancel, || self.provider.upscale(&req, cancel.child_token(), progress)).await?;
        let cost = resp.cost;
        let img = resp.images.into_iter().next().ok_or(crate::providers::ProviderError::Empty)?;
        let bytes = image_bytes(img).await?;
        let (working, master) = decode_blocking(move || decode_upscaled(&bytes, target_side)).await?;
        Ok((working, master, cost))
    }

    /// Like [`Self::generate_with_retry`] but keeps every image the provider returned (for
    /// variations).
    async fn generate_multi_with_retry(
        provider: &dyn ImageProvider,
        req: &ImageRequest,
        retries: usize,
        cancel: tokio_util::sync::CancellationToken,
        mode: SquareMode,
    ) -> Result<(Vec<Vec<u8>>, Vec<Option<MasterPixels>>, Option<f64>), crate::providers::ProviderError> {
        let progress: &crate::providers::ProgressFn = &|_p| {};
        let resp = call_with_retry(retries, &cancel, || provider.generate(req, cancel.child_token(), progress)).await?;
        if resp.images.is_empty() {
            return Err(crate::providers::ProviderError::Empty);
        }
        let mut out = Vec::with_capacity(resp.images.len());
        let mut masters = Vec::with_capacity(resp.images.len());
        for img in resp.images {
            let (work, master) = fetch_and_decode(img, mode).await?;
            out.push(work);
            masters.push(master);
        }
        Ok((out, masters, resp.cost))
    }

    /// Run one provider call with retry, then decode the first image. Returns the map's
    /// working-size RGBA8 pixels, the full-resolution master (if larger than working), and
    /// the cost.
    async fn generate_with_retry(
        provider: &dyn ImageProvider,
        req: &ImageRequest,
        retries: usize,
        cancel: tokio_util::sync::CancellationToken,
        mode: SquareMode,
    ) -> Result<(Vec<u8>, Option<MasterPixels>, Option<f64>), crate::providers::ProviderError> {
        let progress: &crate::providers::ProgressFn = &|_p| {};
        let resp = call_with_retry(retries, &cancel, || provider.generate(req, cancel.child_token(), progress)).await?;
        let img = resp.images.into_iter().next().ok_or(crate::providers::ProviderError::Empty)?;
        let (rgba, master) = fetch_and_decode(img, mode).await?;
        Ok((rgba, master, resp.cost))
    }
}

/// Run `call` with retry/backoff on idempotent failures only (see [`is_retryable`]), capped
/// at `retries` — anything else (including a possibly-billing timeout) returns immediately.
/// The exponential backoff is raced against `cancel`.
async fn call_with_retry<T, Fut>(
    retries: usize,
    cancel: &tokio_util::sync::CancellationToken,
    mut call: impl FnMut() -> Fut,
) -> Result<T, crate::providers::ProviderError>
where
    Fut: std::future::Future<Output = Result<T, crate::providers::ProviderError>>,
{
    let mut attempt = 0;
    loop {
        match call().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempt += 1;
                if !is_retryable(&e) || attempt > retries {
                    return Err(e);
                }
                let backoff = std::time::Duration::from_secs(2u64.saturating_pow(attempt as u32).min(30));
                tokio::select! {
                    () = cancel.cancelled() => return Err(crate::providers::ProviderError::Canceled),
                    () = tokio::time::sleep(backoff) => {}
                }
            }
        }
    }
}

/// Materialize one provider image to bytes: base64/mock data needs no HTTP; a hosted (fal)
/// URL is fetched through the shared download client (fal URLs expire — fetch immediately,
/// off a warm pool).
async fn image_bytes(img: crate::providers::OutputImage) -> Result<Vec<u8>, crate::providers::ProviderError> {
    match img.data {
        crate::providers::ImageData::Base64 { data, .. } => Ok(data),
        u @ crate::providers::ImageData::Url(_) => {
            let http = crate::providers::download_client()?;
            Ok(u.into_bytes(&http).await?.0)
        }
    }
}

/// Run a CPU-bound decode/resample off the runtime workers (a full-image pass is seconds of
/// CPU at master sizes and would stall the reactor).
async fn decode_blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, crate::providers::ProviderError> + Send + 'static,
) -> Result<T, crate::providers::ProviderError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| crate::providers::ProviderError::Job(format!("decode join: {e}")))?
}

/// Fetch one provider image and decode it into `(working, master)`.
async fn fetch_and_decode(img: crate::providers::OutputImage, mode: SquareMode) -> Result<(Vec<u8>, Option<MasterPixels>), crate::providers::ProviderError> {
    let bytes = image_bytes(img).await?;
    decode_blocking(move || decode_to_rgba(&bytes, mode)).await
}

/// The size spec for an image EDIT: pin the output to the reference's square side EXPLICITLY
/// (gpt-image's `auto` can pick a different frame, breaking 1:1 registration with the
/// albedo), rounded down to gpt-image's multiple-of-16 grid. References below the API's
/// pixel floor keep `default`.
fn edit_size_spec(ref_side: usize, default: Option<&ImageSpec>) -> Option<ImageSpec> {
    let side16 = (ref_side / 16) * 16;
    if side16 * side16 >= 655_360 {
        Some(ImageSpec::Pixels {
            width: side16 as u32,
            height: side16 as u32,
        })
    } else {
        default.cloned()
    }
}

/// Encode the provider reference PNG from the best available source (a REAL PNG — providers
/// reject raw RGBA labeled as one). The full-resolution master's deflate takes seconds of
/// CPU, so it runs on a blocking thread; the working-size fallback encodes in milliseconds
/// and stays inline.
async fn encode_reference_png(albedo: &[u8], size: usize, master: Option<MasterPixels>) -> Option<Vec<u8>> {
    match master {
        Some((px, side)) => tokio::task::spawn_blocking(move || crate::maps::encode_png_rgba(&px, side))
            .await
            .ok()
            .flatten(),
        None => crate::maps::encode_png_rgba(albedo, size),
    }
}

/// Whether a failed provider call may be retried by RE-SUBMITTING the request. Only failures
/// that cannot have left a paid job running server-side qualify: pre-submit transport errors
/// and upstream 429/5xx rejections. `Timeout` means the submit→poll deadline expired on a job
/// the server ACCEPTED (it may still be running and billing; fal gets a best-effort cancel),
/// and an HTTP timeout means the upstream may have completed — and billed — the call, so
/// neither is resubmitted: each retry would run a brand-new paid job.
fn is_retryable(e: &crate::providers::ProviderError) -> bool {
    match e {
        crate::providers::ProviderError::Http(e) => !e.is_timeout(),
        crate::providers::ProviderError::Api { status, .. } => *status == 429 || (500..=599).contains(status),
        _ => false,
    }
}

/// The pass chain to reach `target` from `src` under the upscaler's 4× per-pass factor cap,
/// bounded at 3 passes (64× total). Empty when the source already meets the target.
pub fn upscale_chain(src: usize, target: usize) -> Vec<usize> {
    let mut chain = Vec::new();
    let mut side = src.max(1);
    while side < target && chain.len() < 3 {
        side = (side * 4).min(target);
        chain.push(side);
    }
    chain
}

/// How a non-square provider result is normalized to the square pipeline.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SquareMode {
    /// Center-crop (text-to-image: a fresh texture with no reference to register against).
    Crop,
    /// Stretch to the longer side (image edits: 1:1 registration to the reference beats
    /// aspect purity — cropping would cut map content relative to the albedo).
    Stretch,
}

/// Decode an upscaler result into `(working, master)`. An upscale must register 1:1 with its
/// source, so non-square results are STRETCHED, never cropped; the mock's raw RGBA buffer
/// arrives at the requested target side.
fn decode_upscaled(bytes: &[u8], target_side: usize) -> Result<(Vec<u8>, Option<MasterPixels>), crate::providers::ProviderError> {
    let working = crate::maps::TEX_SIZE;
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let (master, side) = crate::maps::stretch_square_rgba(&img);
            let work = crate::maps::resize_square_rgba(&master, side, working);
            let master = (side > working).then_some((master, side));
            Ok((work, master))
        }
        Err(_) if bytes.len() == target_side * target_side * 4 => {
            let work = crate::maps::resize_square_rgba(bytes, target_side, working);
            let master = (target_side > working).then_some((bytes.to_vec(), target_side));
            Ok((work, master))
        }
        Err(e) => Err(crate::providers::ProviderError::Job(format!("upscaled image decode: {e}"))),
    }
}

/// Decode image bytes (PNG/JPEG) into `(working, master)`: the working-size RGBA8 copy that
/// drives the UI, plus the full-resolution square master `(rgba, side)` kept for export
/// quality (None when the source is already at/below the working size). Raw working-size
/// RGBA from the mock provider passes through with no master.
fn decode_to_rgba(bytes: &[u8], mode: SquareMode) -> Result<(Vec<u8>, Option<MasterPixels>), crate::providers::ProviderError> {
    let working = crate::maps::TEX_SIZE;
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let (master, side) = match mode {
                SquareMode::Crop => crate::maps::crop_square_rgba(&img),
                SquareMode::Stretch => crate::maps::stretch_square_rgba(&img),
            };
            let work = crate::maps::resize_square_rgba(&master, side, working);
            let master = (side > working).then_some((master, side));
            Ok((work, master))
        }
        // Not an encoded image: the mock provider's raw RGBA8 buffer at the working size.
        Err(_) if bytes.len() == working * working * 4 => Ok((bytes.to_vec(), None)),
        Err(e) => Err(crate::providers::ProviderError::Job(format!("image decode: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockProvider;
    use std::sync::Arc;

    /// Timeout means an ACCEPTED job hit its deadline — it may still run (and bill)
    /// server-side, so a retry would submit a duplicate paid job. Never resubmit on it.
    #[test]
    fn timeout_is_not_retryable() {
        assert!(!is_retryable(&crate::providers::ProviderError::Timeout(std::time::Duration::from_mins(10))));
    }

    /// Terminal poll failures (`Job`), cancellation, and hard 4xx stay non-retryable;
    /// upstream overload (429/5xx) stays retryable.
    #[test]
    fn retry_classification_matches_idempotency() {
        let api = |status: u16| crate::providers::ProviderError::Api {
            status,
            message: String::new(),
            kind: None,
        };
        assert!(is_retryable(&api(429)), "throttling is safe to retry");
        assert!(is_retryable(&api(503)), "upstream 5xx rejected the submit — safe to retry");
        assert!(!is_retryable(&api(422)), "param errors never heal");
        assert!(!is_retryable(&crate::providers::ProviderError::Job("status poll failed".into())));
        assert!(!is_retryable(&crate::providers::ProviderError::Canceled));
        assert!(!is_retryable(&crate::providers::ProviderError::Empty));
    }

    /// A cancelled batch must stop emitting outcomes — derived maps must not come back
    /// "Ready" after the user pressed Stop.
    #[tokio::test]
    async fn cancelled_batch_emits_no_derived_outcomes() {
        let generator = MapGenerator::new(Arc::new(MockProvider));
        let size = crate::maps::TEX_SIZE;
        let plan = GenerationPlan {
            paths: std::array::from_fn(|_| DerivePath::Derived),
            ..GenerationPlan::default()
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();
        let (tx, mut rx) = mpsc::unbounded_channel();
        generator
            .generate_material(
                vec![0u8; size * size * 4],
                None,
                size,
                MapId::DERIVED.to_vec(),
                PromptContext::default(),
                plan,
                1,
                std::collections::HashMap::new(),
                tx,
                cancel,
            )
            .await;
        assert!(rx.recv().await.is_none(), "a cancelled batch must not emit outcomes");
    }

    /// The chained upscale keeps the last pass's master for the final outcome.
    #[tokio::test]
    async fn upscale_map_via_mock_returns_master_and_preview() {
        let generator = MapGenerator::new(Arc::new(MockProvider));
        let side = crate::maps::TEX_SIZE;
        let (tx, mut rx) = mpsc::unbounded_channel();
        generator
            .upscale_map(
                MapId::Albedo,
                vec![100u8; side * side * 4],
                side,
                side * 4,
                "Standard V2".into(),
                0,
                tx,
                tokio_util::sync::CancellationToken::new(),
            )
            .await;
        let outcome = rx.recv().await.expect("one outcome");
        match outcome {
            MapOutcome::Ready { rgba, master, cost, .. } => {
                assert_eq!(rgba.len(), side * side * 4, "working-size preview refreshed");
                let (mpx, mside) = master.expect("above-working result keeps a master");
                assert_eq!(mside, side * 4);
                assert_eq!(mpx.len(), side * 4 * side * 4 * 4);
                assert_eq!(cost, Some(0.0), "mock upscales are free");
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    /// The pass chain honors the 4× per-pass cap, stops at the target, and never exceeds
    /// three passes.
    #[test]
    fn upscale_chain_respects_factor_cap() {
        assert_eq!(upscale_chain(2880, 4096), vec![4096], "one gentle pass");
        assert_eq!(upscale_chain(400, 4096), vec![1600, 4096], "two passes from a small source");
        assert_eq!(upscale_chain(1024, 16384), vec![4096, 16384], "16K from 1K in two passes");
        assert_eq!(upscale_chain(400, 16384), vec![1600, 6400, 16384], "three passes max");
        assert!(upscale_chain(4096, 4096).is_empty(), "already at target");
        assert!(upscale_chain(8192, 4096).is_empty(), "above target");
    }

    /// A provider result at any resolution/aspect (gpt-image models return e.g. 1024×768)
    /// must land at the square working size — unnormalized pixels crashed the texture upload.
    #[test]
    fn decode_normalizes_any_provider_resolution_to_working_size() {
        let working = crate::maps::TEX_SIZE;
        // A 1024×768 image: blue bands left/right, a red centered 768×768 square.
        let img = image::RgbaImage::from_fn(1024, 768, |x, _| {
            if (128..896).contains(&x) {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();

        let (rgba, master) = decode_to_rgba(&png, SquareMode::Crop).expect("decodes");
        assert_eq!(rgba.len(), working * working * 4, "normalized to the working size");
        // Center-cropped: the blue side bands are gone — corners are red.
        assert_eq!(&rgba[0..3], &[255, 0, 0], "top-left corner should be inside the red center crop");
        let last = rgba.len() - 4;
        assert_eq!(&rgba[last..last + 3], &[255, 0, 0], "bottom-right corner should be red too");
        // The full-resolution master is kept at the cropped native side (768²).
        let (mpx, mside) = master.expect("larger-than-working source keeps a master");
        assert_eq!(mside, 768);
        assert_eq!(mpx.len(), 768 * 768 * 4);
        assert_eq!(&mpx[0..3], &[255, 0, 0]);
    }

    /// The mock provider's raw working-size RGBA buffer passes through untouched (no master).
    #[test]
    fn decode_passes_raw_mock_buffers_through() {
        let working = crate::maps::TEX_SIZE;
        let raw = vec![7u8; working * working * 4];
        let (work, master) = decode_to_rgba(&raw, SquareMode::Crop).expect("raw ok");
        assert_eq!(work, raw);
        assert!(master.is_none());
        // Junk that is neither an image nor a working-size buffer errors instead of crashing.
        assert!(decode_to_rgba(&[1, 2, 3], SquareMode::Crop).is_err());
    }

    /// EDIT results must preserve 1:1 registration: a non-square response is STRETCHED to
    /// square (the reference pipeline's behavior), never cropped — cropping cuts map content
    /// relative to the albedo (the "albedo feels cut" misregistration).
    #[test]
    fn edit_decode_stretches_never_crops() {
        // 1024×768: blue side bands, red center — the bands MUST survive.
        let img = image::RgbaImage::from_fn(1024, 768, |x, _| {
            if (128..896).contains(&x) {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 255])
            }
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let (work, master) = decode_to_rgba(&png, SquareMode::Stretch).expect("decodes");
        let (mpx, mside) = master.expect("master kept");
        assert_eq!(mside, 1024, "stretched to the longer side");
        // Left edge is still blue — nothing was cropped away.
        assert_eq!(&mpx[0..3], &[0, 0, 255], "edge content preserved (stretch, not crop)");
        assert_eq!(&work[0..3], &[0, 0, 255]);
    }

    /// The whole pipeline — orchestrator + concurrency + derive — runs end-to-end against the
    /// offline mock with no network and no key, and every derived + semantic map comes back.
    #[tokio::test]
    async fn generate_material_via_mock_produces_all_maps() {
        let generator = MapGenerator::new(Arc::new(MockProvider));
        let size = crate::maps::TEX_SIZE;
        let layout = crate::maps::build_layout(4821);
        let albedo = crate::maps::render_map(MapId::Albedo, &layout, 4821);
        let (tx, mut rx) = mpsc::unbounded_channel();
        generator
            .generate_material(
                albedo,
                None,
                size,
                MapId::DERIVED.to_vec(),
                PromptContext {
                    base_template: data::BASE_TEMPLATE.into(),
                    final_requirement: data::FINAL_REQUIREMENT.into(),
                    description: "painted hull".into(),
                    map_templates: std::array::from_fn(|_| String::new()),
                    negative: String::new(),
                },
                GenerationPlan::default(),
                4821,
                std::collections::HashMap::new(),
                tx,
                tokio_util::sync::CancellationToken::new(),
            )
            .await;

        let mut ready = std::collections::HashSet::new();
        let mut failed = Vec::new();
        // Drain via recv (the batch already completed, so this returns immediately) rather
        // than try_recv, whose internal expect trips the workspace's disallowed-methods.
        while let Some(outcome) = rx.recv().await {
            match outcome {
                MapOutcome::Ready { map, rgba, .. } => {
                    assert_eq!(rgba.len(), size * size * 4, "{:?} wrong size", map);
                    ready.insert(map);
                }
                MapOutcome::Failed { map, error } => failed.push((map, error)),
                MapOutcome::Variations { .. } => {}
            }
        }
        assert!(failed.is_empty(), "failures: {failed:?}");
        // The default plan: roughness/metallic/emission AI (mock), normal/AO/opacity/
        // displacement derived — all 7 derived maps must be present.
        for id in MapId::DERIVED {
            assert!(ready.contains(&id), "missing map {:?}", id);
        }
    }
}
