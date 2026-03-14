use ::image::{DynamicImage, GenericImageView, ImageBuffer, Rgba};
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Vec2, Vec3};
use half::f16;
use std::sync::{Arc, Mutex};
use wgpu::util::{DeviceExt, TextureDataOrder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneMapper {
    Basic,
    AgX,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CurvePoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HueSatLum {
    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,
}

impl Default for HueSatLum {
    fn default() -> Self {
        Self {
            hue: 0.0,
            saturation: 0.0,
            luminance: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HslSettings {
    pub reds: HueSatLum,
    pub oranges: HueSatLum,
    pub yellows: HueSatLum,
    pub greens: HueSatLum,
    pub aquas: HueSatLum,
    pub blues: HueSatLum,
    pub purples: HueSatLum,
    pub magentas: HueSatLum,
}

impl Default for HslSettings {
    fn default() -> Self {
        Self {
            reds: HueSatLum::default(),
            oranges: HueSatLum::default(),
            yellows: HueSatLum::default(),
            greens: HueSatLum::default(),
            aquas: HueSatLum::default(),
            blues: HueSatLum::default(),
            purples: HueSatLum::default(),
            magentas: HueSatLum::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorGradingSettingsUi {
    pub shadows: HueSatLum,
    pub midtones: HueSatLum,
    pub highlights: HueSatLum,
    pub blending: f32,
    pub balance: f32,
}

impl Default for ColorGradingSettingsUi {
    fn default() -> Self {
        Self {
            shadows: HueSatLum::default(),
            midtones: HueSatLum::default(),
            highlights: HueSatLum::default(),
            blending: 50.0,
            balance: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ColorCalibrationSettingsUi {
    pub shadows_tint: f32,
    pub red_hue: f32,
    pub red_saturation: f32,
    pub green_hue: f32,
    pub green_saturation: f32,
    pub blue_hue: f32,
    pub blue_saturation: f32,
}

impl Default for ColorCalibrationSettingsUi {
    fn default() -> Self {
        Self {
            shadows_tint: 0.0,
            red_hue: 0.0,
            red_saturation: 0.0,
            green_hue: 0.0,
            green_saturation: 0.0,
            blue_hue: 0.0,
            blue_saturation: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CurvesSettings {
    pub luma: Vec<CurvePoint>,
    pub red: Vec<CurvePoint>,
    pub green: Vec<CurvePoint>,
    pub blue: Vec<CurvePoint>,
}

impl Default for CurvesSettings {
    fn default() -> Self {
        Self {
            luma: default_curve_points(),
            red: default_curve_points(),
            green: default_curve_points(),
            blue: default_curve_points(),
        }
    }
}

pub fn default_curve_points() -> Vec<CurvePoint> {
    vec![
        CurvePoint { x: 0.0, y: 0.0 },
        CurvePoint { x: 255.0, y: 255.0 },
    ]
}

#[derive(Debug, Clone)]
pub struct BasicAdjustments {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub tint: f32,
    pub vibrance: f32,
    pub sharpness: f32,
    pub luma_noise_reduction: f32,
    pub color_noise_reduction: f32,
    pub clarity: f32,
    pub dehaze: f32,
    pub structure: f32,
    pub centre: f32,
    pub chromatic_aberration_red_cyan: f32,
    pub chromatic_aberration_blue_yellow: f32,
    pub vignette_amount: f32,
    pub vignette_midpoint: f32,
    pub vignette_roundness: f32,
    pub vignette_feather: f32,
    pub grain_amount: f32,
    pub grain_size: f32,
    pub grain_roughness: f32,
    pub glow_amount: f32,
    pub halation_amount: f32,
    pub flare_amount: f32,
    pub tone_mapper: ToneMapper,
    pub hsl: HslSettings,
    pub color_grading: ColorGradingSettingsUi,
    pub color_calibration: ColorCalibrationSettingsUi,
    pub curves: CurvesSettings,
}

impl Default for BasicAdjustments {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            saturation: 0.0,
            temperature: 0.0,
            tint: 0.0,
            vibrance: 0.0,
            sharpness: 0.0,
            luma_noise_reduction: 0.0,
            color_noise_reduction: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
            structure: 0.0,
            centre: 0.0,
            chromatic_aberration_red_cyan: 0.0,
            chromatic_aberration_blue_yellow: 0.0,
            vignette_amount: 0.0,
            vignette_midpoint: 50.0,
            vignette_roundness: 0.0,
            vignette_feather: 50.0,
            grain_amount: 0.0,
            grain_size: 25.0,
            grain_roughness: 50.0,
            glow_amount: 0.0,
            halation_amount: 0.0,
            flare_amount: 0.0,
            tone_mapper: ToneMapper::AgX,
            hsl: HslSettings::default(),
            color_grading: ColorGradingSettingsUi::default(),
            color_calibration: ColorCalibrationSettingsUi::default(),
            curves: CurvesSettings::default(),
        }
    }
}

#[derive(Clone)]
pub struct RapidRawRenderer {
    inner: Arc<Mutex<RendererInner>>,
}

struct RendererInner {
    context: GpuContext,
    pipeline: MainPipeline,
}

impl RapidRawRenderer {
    pub fn new() -> Result<Self, String> {
        let context = init_gpu_context()?;
        let pipeline = MainPipeline::new(&context)?;
        Ok(Self {
            inner: Arc::new(Mutex::new(RendererInner { context, pipeline })),
        })
    }

    pub fn render(
        &self,
        base_image: &DynamicImage,
        adjustments: &BasicAdjustments,
        is_raw: bool,
    ) -> Result<DynamicImage, String> {
        let mut inner = self.inner.lock().map_err(|_| "Renderer lock poisoned".to_string())?;
        let context = inner.context.clone();
        inner.pipeline.render(&context, base_image, adjustments, is_raw)
    }
}

#[derive(Clone)]
struct GpuContext {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    limits: wgpu::Limits,
}

fn init_gpu_context() -> Result<GpuContext, String> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .map_err(|error| format!("Failed to find a wgpu adapter: {error}"))?;

    let limits = adapter.limits();
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("RapidRAW Iced POC Device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits.clone(),
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        },
    ))
    .map_err(|error| format!("Failed to create device: {error}"))?;

    Ok(GpuContext {
        device: Arc::new(device),
        queue: Arc::new(queue),
        limits,
    })
}

struct MainPipeline {
    blur_bgl: wgpu::BindGroupLayout,
    h_blur_pipeline: wgpu::ComputePipeline,
    v_blur_pipeline: wgpu::ComputePipeline,
    blur_params_buffer: wgpu::Buffer,
    flare_bgl_0: wgpu::BindGroupLayout,
    flare_bgl_1: wgpu::BindGroupLayout,
    flare_threshold_pipeline: wgpu::ComputePipeline,
    flare_ghosts_pipeline: wgpu::ComputePipeline,
    flare_params_buffer: wgpu::Buffer,
    flare_threshold_view: wgpu::TextureView,
    flare_ghosts_view: wgpu::TextureView,
    flare_final_view: wgpu::TextureView,
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::ComputePipeline,
    adjustments_buffer: wgpu::Buffer,
    dummy_mask_view: wgpu::TextureView,
    dummy_blur_view: wgpu::TextureView,
    dummy_flare_view: wgpu::TextureView,
    dummy_lut_view: wgpu::TextureView,
    lut_sampler: wgpu::Sampler,
    flare_sampler: wgpu::Sampler,
}

impl MainPipeline {
    fn new(context: &GpuContext) -> Result<Self, String> {
        let device = &context.device;
        const MAX_MASKS: u32 = 8;
        const FLARE_MAP_SIZE: u32 = 512;

        let blur_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RapidRAW Blur Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../src-tauri/src/shaders/blur.wgsl").into(),
            ),
        });

        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RapidRAW Blur BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RapidRAW Blur Pipeline Layout"),
            bind_group_layouts: &[&blur_bgl],
            immediate_size: 0,
        });

        let h_blur_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("RapidRAW Horizontal Blur"),
            layout: Some(&blur_pipeline_layout),
            module: &blur_shader_module,
            entry_point: Some("horizontal_blur"),
            compilation_options: Default::default(),
            cache: None,
        });

        let v_blur_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("RapidRAW Vertical Blur"),
            layout: Some(&blur_pipeline_layout),
            module: &blur_shader_module,
            entry_point: Some("vertical_blur"),
            compilation_options: Default::default(),
            cache: None,
        });

        let blur_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("RapidRAW Blur Params"),
            size: std::mem::size_of::<BlurParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let flare_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RapidRAW Flare Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../src-tauri/src/shaders/flare.wgsl").into(),
            ),
        });

        let flare_bgl_0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RapidRAW Flare BGL 0"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let flare_bgl_1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RapidRAW Flare BGL 1"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

        let flare_threshold_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RapidRAW Flare Threshold Layout"),
            bind_group_layouts: &[&flare_bgl_0],
            immediate_size: 0,
        });
        let flare_ghosts_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RapidRAW Flare Ghosts Layout"),
            bind_group_layouts: &[&flare_bgl_0, &flare_bgl_1],
            immediate_size: 0,
        });

        let flare_threshold_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("RapidRAW Flare Threshold Pipeline"),
                layout: Some(&flare_threshold_layout),
                module: &flare_shader,
                entry_point: Some("threshold_main"),
                compilation_options: Default::default(),
                cache: None,
            });
        let flare_ghosts_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("RapidRAW Flare Ghosts Pipeline"),
                layout: Some(&flare_ghosts_layout),
                module: &flare_shader,
                entry_point: Some("ghosts_main"),
                compilation_options: Default::default(),
                cache: None,
            });

        let flare_params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("RapidRAW Flare Params"),
            size: std::mem::size_of::<FlareParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let flare_tex_desc = wgpu::TextureDescriptor {
            label: Some("RapidRAW Flare Texture"),
            size: wgpu::Extent3d {
                width: FLARE_MAP_SIZE,
                height: FLARE_MAP_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };
        let flare_threshold_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Flare Threshold Texture"),
            ..flare_tex_desc
        });
        let flare_threshold_view = flare_threshold_texture.create_view(&Default::default());
        let flare_ghosts_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Flare Ghosts Texture"),
            ..flare_tex_desc
        });
        let flare_ghosts_view = flare_ghosts_texture.create_view(&Default::default());
        let flare_final_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Flare Final Texture"),
            ..flare_tex_desc
        });
        let flare_final_view = flare_final_texture.create_view(&Default::default());

        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("RapidRAW Main Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../src-tauri/src/shaders/shader.wgsl").into(),
            ),
        });

        let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ];

        for i in 0..MAX_MASKS {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding: 3 + i,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }

        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3 + MAX_MASKS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 4 + MAX_MASKS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
            count: None,
        });

        for binding in [5 + MAX_MASKS, 6 + MAX_MASKS, 7 + MAX_MASKS, 8 + MAX_MASKS] {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }

        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 9 + MAX_MASKS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 10 + MAX_MASKS,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("RapidRAW Main BGL"),
            entries: &entries,
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("RapidRAW Main Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("RapidRAW Main Pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let adjustments_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("RapidRAW Adjustments Buffer"),
            size: std::mem::size_of::<AllAdjustments>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let dummy_rgba_desc = wgpu::TextureDescriptor {
            label: Some("RapidRAW Dummy RGBA"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let dummy_blur_texture = device.create_texture(&dummy_rgba_desc);
        let dummy_blur_view = dummy_blur_texture.create_view(&Default::default());

        let dummy_flare_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Dummy Flare"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_flare_view = dummy_flare_texture.create_view(&Default::default());

        let dummy_mask_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Dummy Mask"),
            format: wgpu::TextureFormat::R8Unorm,
            ..dummy_rgba_desc
        });
        let dummy_mask_view = dummy_mask_texture.create_view(&Default::default());

        let dummy_lut_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Dummy LUT"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let dummy_lut_view = dummy_lut_texture.create_view(&Default::default());

        let lut_sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());
        let flare_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            blur_bgl,
            h_blur_pipeline,
            v_blur_pipeline,
            blur_params_buffer,
            flare_bgl_0,
            flare_bgl_1,
            flare_threshold_pipeline,
            flare_ghosts_pipeline,
            flare_params_buffer,
            flare_threshold_view,
            flare_ghosts_view,
            flare_final_view,
            bind_group_layout,
            pipeline,
            adjustments_buffer,
            dummy_mask_view,
            dummy_blur_view,
            dummy_flare_view,
            dummy_lut_view,
            lut_sampler,
            flare_sampler,
        })
    }

    fn render(
        &mut self,
        context: &GpuContext,
        base_image: &DynamicImage,
        adjustments: &BasicAdjustments,
        is_raw: bool,
    ) -> Result<DynamicImage, String> {
        let (width, height) = base_image.dimensions();
        if width > context.limits.max_texture_dimension_2d || height > context.limits.max_texture_dimension_2d {
            return Err("Image exceeds GPU texture limits".to_string());
        }

        let device = &context.device;
        let queue = &context.queue;

        let input_texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("RapidRAW Input Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            TextureDataOrder::MipMajor,
            bytemuck::cast_slice(&to_rgba_f16(base_image)),
        );
        let input_view = input_texture.create_view(&Default::default());

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Output Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&Default::default());

        let blur_texture_desc = wgpu::TextureDescriptor {
            label: Some("RapidRAW Blur Texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        };
        let ping_pong_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Blur PingPong"),
            ..blur_texture_desc
        });
        let ping_pong_view = ping_pong_texture.create_view(&Default::default());
        let sharpness_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Sharpness Blur"),
            ..blur_texture_desc
        });
        let sharpness_blur_view = sharpness_blur_texture.create_view(&Default::default());
        let tonal_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Tonal Blur"),
            ..blur_texture_desc
        });
        let tonal_blur_view = tonal_blur_texture.create_view(&Default::default());
        let clarity_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Clarity Blur"),
            ..blur_texture_desc
        });
        let clarity_blur_view = clarity_blur_texture.create_view(&Default::default());
        let structure_blur_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("RapidRAW Structure Blur"),
            ..blur_texture_desc
        });
        let structure_blur_view = structure_blur_texture.create_view(&Default::default());

        let all_adjustments = build_all_adjustments(adjustments, is_raw);
        queue.write_buffer(&self.adjustments_buffer, 0, bytemuck::bytes_of(&all_adjustments));

        let run_blur = |radius: u32, output_view: &wgpu::TextureView| -> bool {
            if radius == 0 {
                return false;
            }

            let params = BlurParams {
                radius,
                tile_offset_x: 0,
                tile_offset_y: 0,
                input_width: width,
                input_height: height,
                _pad1: 0,
                _pad2: 0,
                _pad3: 0,
            };
            queue.write_buffer(&self.blur_params_buffer, 0, bytemuck::bytes_of(&params));

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RapidRAW Blur Encoder"),
            });

            let h_blur_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RapidRAW H-Blur BG"),
                layout: &self.blur_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&ping_pong_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.blur_params_buffer.as_entire_binding(),
                    },
                ],
            });

            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("RapidRAW H-Blur Pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.h_blur_pipeline);
                cpass.set_bind_group(0, &h_blur_bg, &[]);
                cpass.dispatch_workgroups((width + 255) / 256, height, 1);
            }

            let v_blur_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RapidRAW V-Blur BG"),
                layout: &self.blur_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&ping_pong_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(output_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.blur_params_buffer.as_entire_binding(),
                    },
                ],
            });

            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("RapidRAW V-Blur Pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.v_blur_pipeline);
                cpass.set_bind_group(0, &v_blur_bg, &[]);
                cpass.dispatch_workgroups(width, (height + 255) / 256, 1);
            }

            queue.submit(Some(encoder.finish()));
            true
        };

        let did_create_sharpness_blur = run_blur(1, &sharpness_blur_view);
        let did_create_tonal_blur = run_blur(4, &tonal_blur_view);
        let did_create_clarity_blur = run_blur(8, &clarity_blur_view);
        let did_create_structure_blur = run_blur(40, &structure_blur_view);
        let use_flare = if adjustments.flare_amount > 0.0 {
            const FLARE_MAP_SIZE: u32 = 512;

            let aspect_ratio = if height > 0 {
                width as f32 / height as f32
            } else {
                1.0
            };
            let flare_params = FlareParams {
                amount: all_adjustments.global.flare_amount,
                is_raw: all_adjustments.global.is_raw_image,
                exposure: all_adjustments.global.exposure,
                brightness: all_adjustments.global.brightness,
                contrast: all_adjustments.global.contrast,
                whites: all_adjustments.global.whites,
                aspect_ratio,
                _pad: 0.0,
            };
            queue.write_buffer(&self.flare_params_buffer, 0, bytemuck::bytes_of(&flare_params));

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RapidRAW Flare Encoder"),
            });

            let bg0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RapidRAW Flare BG0"),
                layout: &self.flare_bgl_0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_threshold_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.flare_params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.flare_sampler),
                    },
                ],
            });

            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("RapidRAW Flare Threshold Pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.flare_threshold_pipeline);
                cpass.set_bind_group(0, &bg0, &[]);
                cpass.dispatch_workgroups(FLARE_MAP_SIZE / 16, FLARE_MAP_SIZE / 16, 1);
            }

            let bg0_ghosts = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RapidRAW Flare Ghosts BG0"),
                layout: &self.flare_bgl_0,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&input_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_final_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.flare_params_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.flare_sampler),
                    },
                ],
            });

            let bg1 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RapidRAW Flare Ghosts BG1"),
                layout: &self.flare_bgl_1,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.flare_threshold_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_ghosts_view),
                    },
                ],
            });

            {
                let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("RapidRAW Flare Ghosts Pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.flare_ghosts_pipeline);
                cpass.set_bind_group(0, &bg0_ghosts, &[]);
                cpass.set_bind_group(1, &bg1, &[]);
                cpass.dispatch_workgroups(FLARE_MAP_SIZE / 16, FLARE_MAP_SIZE / 16, 1);
            }

            queue.submit(Some(encoder.finish()));

            let params = BlurParams {
                radius: 12,
                tile_offset_x: 0,
                tile_offset_y: 0,
                input_width: FLARE_MAP_SIZE,
                input_height: FLARE_MAP_SIZE,
                _pad1: 0,
                _pad2: 0,
                _pad3: 0,
            };
            queue.write_buffer(&self.blur_params_buffer, 0, bytemuck::bytes_of(&params));

            let mut blur_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("RapidRAW Flare Blur Encoder"),
            });
            let h_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RapidRAW Flare Blur H"),
                layout: &self.blur_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.flare_ghosts_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_threshold_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.blur_params_buffer.as_entire_binding(),
                    },
                ],
            });
            {
                let mut cpass = blur_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("RapidRAW Flare Blur H Pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.h_blur_pipeline);
                cpass.set_bind_group(0, &h_bg, &[]);
                cpass.dispatch_workgroups(FLARE_MAP_SIZE / 256 + 1, FLARE_MAP_SIZE, 1);
            }

            let v_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("RapidRAW Flare Blur V"),
                layout: &self.blur_bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.flare_threshold_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.flare_final_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.blur_params_buffer.as_entire_binding(),
                    },
                ],
            });
            {
                let mut cpass = blur_encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("RapidRAW Flare Blur V Pass"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.v_blur_pipeline);
                cpass.set_bind_group(0, &v_bg, &[]);
                cpass.dispatch_workgroups(FLARE_MAP_SIZE, FLARE_MAP_SIZE / 256 + 1, 1);
            }
            queue.submit(Some(blur_encoder.finish()));
            true
        } else {
            false
        };

        const MAX_MASKS: usize = 8;
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&input_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&output_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: self.adjustments_buffer.as_entire_binding(),
            },
        ];

        for i in 0..MAX_MASKS {
            entries.push(wgpu::BindGroupEntry {
                binding: 3 + i as u32,
                resource: wgpu::BindingResource::TextureView(&self.dummy_mask_view),
            });
        }

        entries.push(wgpu::BindGroupEntry {
            binding: 11,
            resource: wgpu::BindingResource::TextureView(&self.dummy_lut_view),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 12,
            resource: wgpu::BindingResource::Sampler(&self.lut_sampler),
        });

        entries.push(wgpu::BindGroupEntry {
            binding: 13,
            resource: wgpu::BindingResource::TextureView(if did_create_sharpness_blur {
                &sharpness_blur_view
            } else {
                &self.dummy_blur_view
            }),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 14,
            resource: wgpu::BindingResource::TextureView(if did_create_tonal_blur {
                &tonal_blur_view
            } else {
                &self.dummy_blur_view
            }),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 15,
            resource: wgpu::BindingResource::TextureView(if did_create_clarity_blur {
                &clarity_blur_view
            } else {
                &self.dummy_blur_view
            }),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 16,
            resource: wgpu::BindingResource::TextureView(if did_create_structure_blur {
                &structure_blur_view
            } else {
                &self.dummy_blur_view
            }),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 17,
            resource: wgpu::BindingResource::TextureView(if use_flare {
                &self.flare_final_view
            } else {
                &self.dummy_flare_view
            }),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 18,
            resource: wgpu::BindingResource::Sampler(&self.flare_sampler),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("RapidRAW Main Bind Group"),
            layout: &self.bind_group_layout,
            entries: &entries,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("RapidRAW Main Encoder"),
        });

        {
            let mut compute = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("RapidRAW Main Pass"),
                timestamp_writes: None,
            });
            compute.set_pipeline(&self.pipeline);
            compute.set_bind_group(0, &bind_group, &[]);
            compute.dispatch_workgroups((width + 7) / 8, (height + 7) / 8, 1);
        }

        let readback = create_readback_buffer(device, width, height);
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(readback.padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        queue.submit(Some(encoder.finish()));

        let pixels = map_readback(device, &readback, width, height)?;
        let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, pixels)
            .ok_or_else(|| "Failed to build image buffer from shader output".to_string())?;
        Ok(DynamicImage::ImageRgba8(image))
    }
}

struct ReadbackBuffer {
    buffer: wgpu::Buffer,
    padded_bytes_per_row: u32,
}

fn create_readback_buffer(device: &wgpu::Device, width: u32, height: u32) -> ReadbackBuffer {
    let unpadded_bytes_per_row = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = (unpadded_bytes_per_row + align - 1) & !(align - 1);
    let size = padded_bytes_per_row as u64 * height as u64;

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("RapidRAW Readback Buffer"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    ReadbackBuffer {
        buffer,
        padded_bytes_per_row,
    }
}

fn map_readback(
    device: &wgpu::Device,
    readback: &ReadbackBuffer,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let slice = readback.buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::from_secs(30)),
    });
    rx.recv()
        .map_err(|_| "Failed to receive GPU readback".to_string())?
        .map_err(|error| error.to_string())?;

    let padded = slice.get_mapped_range();
    let unpadded_bytes_per_row = (width * 4) as usize;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in padded.chunks(readback.padded_bytes_per_row as usize).take(height as usize) {
        pixels.extend_from_slice(&row[..unpadded_bytes_per_row]);
    }
    drop(padded);
    readback.buffer.unmap();
    Ok(pixels)
}

fn to_rgba_f16(image: &DynamicImage) -> Vec<f16> {
    image
        .to_rgba32f()
        .into_raw()
        .into_iter()
        .map(f16::from_f32)
        .collect()
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct Point {
    x: f32,
    y: f32,
    _pad1: f32,
    _pad2: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct BlurParams {
    radius: u32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    input_width: u32,
    input_height: u32,
    _pad1: u32,
    _pad2: u32,
    _pad3: u32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct FlareParams {
    amount: f32,
    is_raw: u32,
    exposure: f32,
    brightness: f32,
    contrast: f32,
    whites: f32,
    aspect_ratio: f32,
    _pad: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct HslColor {
    hue: f32,
    saturation: f32,
    luminance: f32,
    _pad: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct ColorGradeSettings {
    hue: f32,
    saturation: f32,
    luminance: f32,
    _pad: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct ColorCalibrationSettings {
    shadows_tint: f32,
    red_hue: f32,
    red_saturation: f32,
    green_hue: f32,
    green_saturation: f32,
    blue_hue: f32,
    blue_saturation: f32,
    _pad1: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable)]
#[repr(C)]
struct GpuMat3 {
    col0: [f32; 4],
    col1: [f32; 4],
    col2: [f32; 4],
}

impl Default for GpuMat3 {
    fn default() -> Self {
        Self {
            col0: [1.0, 0.0, 0.0, 0.0],
            col1: [0.0, 1.0, 0.0, 0.0],
            col2: [0.0, 0.0, 1.0, 0.0],
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct GlobalAdjustments {
    exposure: f32,
    brightness: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    saturation: f32,
    temperature: f32,
    tint: f32,
    vibrance: f32,
    sharpness: f32,
    luma_noise_reduction: f32,
    color_noise_reduction: f32,
    clarity: f32,
    dehaze: f32,
    structure: f32,
    centré: f32,
    vignette_amount: f32,
    vignette_midpoint: f32,
    vignette_roundness: f32,
    vignette_feather: f32,
    grain_amount: f32,
    grain_size: f32,
    grain_roughness: f32,
    chromatic_aberration_red_cyan: f32,
    chromatic_aberration_blue_yellow: f32,
    show_clipping: u32,
    is_raw_image: u32,
    _pad_ca1: f32,
    has_lut: u32,
    lut_intensity: f32,
    tonemapper_mode: u32,
    _pad_lut2: f32,
    _pad_lut3: f32,
    _pad_lut4: f32,
    _pad_lut5: f32,
    _pad_agx1: f32,
    _pad_agx2: f32,
    _pad_agx3: f32,
    agx_pipe_to_rendering_matrix: GpuMat3,
    agx_rendering_to_pipe_matrix: GpuMat3,
    _pad_cg1: f32,
    _pad_cg2: f32,
    _pad_cg3: f32,
    _pad_cg4: f32,
    color_grading_shadows: ColorGradeSettings,
    color_grading_midtones: ColorGradeSettings,
    color_grading_highlights: ColorGradeSettings,
    color_grading_blending: f32,
    color_grading_balance: f32,
    _pad2: f32,
    _pad3: f32,
    color_calibration: ColorCalibrationSettings,
    hsl: [HslColor; 8],
    luma_curve: [Point; 16],
    red_curve: [Point; 16],
    green_curve: [Point; 16],
    blue_curve: [Point; 16],
    luma_curve_count: u32,
    red_curve_count: u32,
    green_curve_count: u32,
    blue_curve_count: u32,
    _pad_end1: f32,
    _pad_end2: f32,
    _pad_end3: f32,
    _pad_end4: f32,
    glow_amount: f32,
    halation_amount: f32,
    flare_amount: f32,
    _pad_creative_1: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct MaskAdjustments {
    exposure: f32,
    brightness: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    saturation: f32,
    temperature: f32,
    tint: f32,
    vibrance: f32,
    sharpness: f32,
    luma_noise_reduction: f32,
    color_noise_reduction: f32,
    clarity: f32,
    dehaze: f32,
    structure: f32,
    glow_amount: f32,
    halation_amount: f32,
    flare_amount: f32,
    _pad1: f32,
    _pad_cg1: f32,
    _pad_cg2: f32,
    _pad_cg3: f32,
    color_grading_shadows: ColorGradeSettings,
    color_grading_midtones: ColorGradeSettings,
    color_grading_highlights: ColorGradeSettings,
    color_grading_blending: f32,
    color_grading_balance: f32,
    _pad5: f32,
    _pad6: f32,
    hsl: [HslColor; 8],
    luma_curve: [Point; 16],
    red_curve: [Point; 16],
    green_curve: [Point; 16],
    blue_curve: [Point; 16],
    luma_curve_count: u32,
    red_curve_count: u32,
    green_curve_count: u32,
    blue_curve_count: u32,
    _pad_end4: f32,
    _pad_end5: f32,
    _pad_end6: f32,
    _pad_end7: f32,
}

#[derive(Debug, Clone, Copy, Pod, Zeroable, Default)]
#[repr(C)]
struct AllAdjustments {
    global: GlobalAdjustments,
    mask_adjustments: [MaskAdjustments; 9],
    mask_count: u32,
    tile_offset_x: u32,
    tile_offset_y: u32,
    mask_atlas_cols: u32,
}

fn build_all_adjustments(adjustments: &BasicAdjustments, is_raw: bool) -> AllAdjustments {
    let (pipe_to_rendering, rendering_to_pipe) = calculate_agx_matrices();
    let mut all = AllAdjustments::default();
    let (luma_curve, luma_curve_count) = convert_curve_points(&adjustments.curves.luma);
    let (red_curve, red_curve_count) = convert_curve_points(&adjustments.curves.red);
    let (green_curve, green_curve_count) = convert_curve_points(&adjustments.curves.green);
    let (blue_curve, blue_curve_count) = convert_curve_points(&adjustments.curves.blue);
    all.global.exposure = adjustments.exposure / 0.8;
    all.global.brightness = adjustments.brightness / 0.8;
    all.global.contrast = adjustments.contrast / 100.0;
    all.global.highlights = adjustments.highlights / 150.0;
    all.global.shadows = adjustments.shadows / 100.0;
    all.global.whites = adjustments.whites / 30.0;
    all.global.blacks = adjustments.blacks / 60.0;
    all.global.saturation = adjustments.saturation / 100.0;
    all.global.temperature = adjustments.temperature / 25.0;
    all.global.tint = adjustments.tint / 100.0;
    all.global.vibrance = adjustments.vibrance / 100.0;
    all.global.sharpness = adjustments.sharpness / 40.0;
    all.global.luma_noise_reduction = adjustments.luma_noise_reduction / 100.0;
    all.global.color_noise_reduction = adjustments.color_noise_reduction / 100.0;
    all.global.clarity = adjustments.clarity / 200.0;
    all.global.dehaze = adjustments.dehaze / 750.0;
    all.global.structure = adjustments.structure / 200.0;
    all.global.centré = adjustments.centre / 250.0;
    all.global.chromatic_aberration_red_cyan =
        adjustments.chromatic_aberration_red_cyan / 10000.0;
    all.global.chromatic_aberration_blue_yellow =
        adjustments.chromatic_aberration_blue_yellow / 10000.0;
    all.global.vignette_amount = adjustments.vignette_amount / 100.0;
    all.global.vignette_midpoint = adjustments.vignette_midpoint / 100.0;
    all.global.vignette_roundness = adjustments.vignette_roundness / 100.0;
    all.global.vignette_feather = adjustments.vignette_feather / 100.0;
    all.global.grain_amount = adjustments.grain_amount / 200.0;
    all.global.grain_size = adjustments.grain_size / 50.0;
    all.global.grain_roughness = adjustments.grain_roughness / 100.0;
    all.global.is_raw_image = if is_raw { 1 } else { 0 };
    all.global.tonemapper_mode = if matches!(adjustments.tone_mapper, ToneMapper::AgX) {
        1
    } else {
        0
    };
    all.global.agx_pipe_to_rendering_matrix = pipe_to_rendering;
    all.global.agx_rendering_to_pipe_matrix = rendering_to_pipe;
    all.global.color_grading_shadows = convert_color_grade(adjustments.color_grading.shadows);
    all.global.color_grading_midtones = convert_color_grade(adjustments.color_grading.midtones);
    all.global.color_grading_highlights = convert_color_grade(adjustments.color_grading.highlights);
    all.global.color_grading_blending = adjustments.color_grading.blending / 100.0;
    all.global.color_grading_balance = adjustments.color_grading.balance / 200.0;
    all.global.color_calibration = ColorCalibrationSettings {
        shadows_tint: adjustments.color_calibration.shadows_tint / 400.0,
        red_hue: adjustments.color_calibration.red_hue / 400.0,
        red_saturation: adjustments.color_calibration.red_saturation / 120.0,
        green_hue: adjustments.color_calibration.green_hue / 400.0,
        green_saturation: adjustments.color_calibration.green_saturation / 120.0,
        blue_hue: adjustments.color_calibration.blue_hue / 400.0,
        blue_saturation: adjustments.color_calibration.blue_saturation / 120.0,
        _pad1: 0.0,
    };
    all.global.hsl = convert_hsl_settings(&adjustments.hsl);
    all.global.luma_curve = luma_curve;
    all.global.red_curve = red_curve;
    all.global.green_curve = green_curve;
    all.global.blue_curve = blue_curve;
    all.global.luma_curve_count = luma_curve_count;
    all.global.red_curve_count = red_curve_count;
    all.global.green_curve_count = green_curve_count;
    all.global.blue_curve_count = blue_curve_count;
    all.global.glow_amount = adjustments.glow_amount / 100.0;
    all.global.halation_amount = adjustments.halation_amount / 100.0;
    all.global.flare_amount = adjustments.flare_amount / 100.0;
    all
}

fn convert_color_grade(value: HueSatLum) -> ColorGradeSettings {
    ColorGradeSettings {
        hue: value.hue,
        saturation: value.saturation / 500.0,
        luminance: value.luminance / 500.0,
        _pad: 0.0,
    }
}

fn convert_hsl_settings(value: &HslSettings) -> [HslColor; 8] {
    [
        convert_hsl(value.reds),
        convert_hsl(value.oranges),
        convert_hsl(value.yellows),
        convert_hsl(value.greens),
        convert_hsl(value.aquas),
        convert_hsl(value.blues),
        convert_hsl(value.purples),
        convert_hsl(value.magentas),
    ]
}

fn convert_hsl(value: HueSatLum) -> HslColor {
    HslColor {
        hue: value.hue * 0.3,
        saturation: value.saturation / 100.0,
        luminance: value.luminance / 100.0,
        _pad: 0.0,
    }
}

fn convert_curve_points(points: &[CurvePoint]) -> ([Point; 16], u32) {
    let mut aligned = [Point::default(); 16];
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));

    let count = sorted.len().clamp(0, 16);
    for (index, point) in sorted.into_iter().take(16).enumerate() {
        aligned[index] = Point {
            x: point.x.clamp(0.0, 255.0),
            y: point.y.clamp(0.0, 255.0),
            _pad1: 0.0,
            _pad2: 0.0,
        };
    }

    (aligned, count as u32)
}

const WP_D65: Vec2 = Vec2::new(0.3127, 0.3290);
const PRIMARIES_SRGB: [Vec2; 3] = [
    Vec2::new(0.64, 0.33),
    Vec2::new(0.30, 0.60),
    Vec2::new(0.15, 0.06),
];
const PRIMARIES_REC2020: [Vec2; 3] = [
    Vec2::new(0.708, 0.292),
    Vec2::new(0.170, 0.797),
    Vec2::new(0.131, 0.046),
];

fn xy_to_xyz(xy: Vec2) -> Vec3 {
    if xy.y < 1e-6 {
        Vec3::ZERO
    } else {
        Vec3::new(xy.x / xy.y, 1.0, (1.0 - xy.x - xy.y) / xy.y)
    }
}

fn primaries_to_xyz_matrix(primaries: &[Vec2; 3], white_point: Vec2) -> Mat3 {
    let r_xyz = xy_to_xyz(primaries[0]);
    let g_xyz = xy_to_xyz(primaries[1]);
    let b_xyz = xy_to_xyz(primaries[2]);
    let primaries_matrix = Mat3::from_cols(r_xyz, g_xyz, b_xyz);
    let white_point_xyz = xy_to_xyz(white_point);
    let s = primaries_matrix.inverse() * white_point_xyz;
    Mat3::from_cols(r_xyz * s.x, g_xyz * s.y, b_xyz * s.z)
}

fn rotate_and_scale_primary(primary: Vec2, white_point: Vec2, scale: f32, rotation: f32) -> Vec2 {
    let p_rel = primary - white_point;
    let p_scaled = p_rel * scale;
    let (sin_r, cos_r) = rotation.sin_cos();
    let p_rotated = Vec2::new(
        p_scaled.x * cos_r - p_scaled.y * sin_r,
        p_scaled.x * sin_r + p_scaled.y * cos_r,
    );
    white_point + p_rotated
}

fn mat3_to_gpu_mat3(m: Mat3) -> GpuMat3 {
    GpuMat3 {
        col0: [m.x_axis.x, m.x_axis.y, m.x_axis.z, 0.0],
        col1: [m.y_axis.x, m.y_axis.y, m.y_axis.z, 0.0],
        col2: [m.z_axis.x, m.z_axis.y, m.z_axis.z, 0.0],
    }
}

fn calculate_agx_matrices() -> (GpuMat3, GpuMat3) {
    let pipe_work_profile_to_xyz = primaries_to_xyz_matrix(&PRIMARIES_SRGB, WP_D65);
    let base_profile_to_xyz = primaries_to_xyz_matrix(&PRIMARIES_REC2020, WP_D65);
    let xyz_to_base_profile = base_profile_to_xyz.inverse();
    let pipe_to_base = xyz_to_base_profile * pipe_work_profile_to_xyz;

    let inset = [0.29462451, 0.25861925, 0.14641371];
    let rotation = [0.03540329, -0.02108586, -0.06305724];
    let outset = [0.290776401758, 0.263155400753, 0.045810721815];
    let unrotation = [0.03540329, -0.02108586, -0.06305724];

    let mut inset_and_rotated_primaries = [Vec2::ZERO; 3];
    for i in 0..3 {
        inset_and_rotated_primaries[i] =
            rotate_and_scale_primary(PRIMARIES_REC2020[i], WP_D65, 1.0 - inset[i], rotation[i]);
    }
    let rendering_to_xyz = primaries_to_xyz_matrix(&inset_and_rotated_primaries, WP_D65);
    let base_to_rendering = xyz_to_base_profile * rendering_to_xyz;

    let mut outset_and_unrotated_primaries = [Vec2::ZERO; 3];
    for i in 0..3 {
        outset_and_unrotated_primaries[i] =
            rotate_and_scale_primary(PRIMARIES_REC2020[i], WP_D65, 1.0 - outset[i], 0.0 * unrotation[i]);
    }
    let outset_to_xyz = primaries_to_xyz_matrix(&outset_and_unrotated_primaries, WP_D65);
    let temp_matrix = xyz_to_base_profile * outset_to_xyz;
    let rendering_to_base = temp_matrix.inverse();

    let pipe_to_rendering = base_to_rendering * pipe_to_base;
    let rendering_to_pipe = pipe_to_base.inverse() * rendering_to_base;

    (
        mat3_to_gpu_mat3(pipe_to_rendering),
        mat3_to_gpu_mat3(rendering_to_pipe),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::{DynamicImage, Rgba, RgbaImage};

    #[test]
    fn exposure_changes_output_pixels() {
        let renderer = RapidRawRenderer::new().expect("renderer init");
        let image = DynamicImage::ImageRgba8(RgbaImage::from_fn(4, 4, |x, y| {
            let v = ((x + y) * 24 + 32) as u8;
            Rgba([v, v.saturating_add(10), v.saturating_add(20), 255])
        }));

        let base = renderer
            .render(&image, &BasicAdjustments::default(), false)
            .expect("base render");

        let adjusted = renderer
            .render(
                &image,
                &BasicAdjustments {
                    exposure: 1.0,
                    ..BasicAdjustments::default()
                },
                false,
            )
            .expect("adjusted render");

        assert_ne!(base.to_rgba8().into_raw(), adjusted.to_rgba8().into_raw());
    }
}
