//! GPU backdrop blur with compositor-level rounded clipping, not a window accent.
//! Shared GPUI foreground stays sharp above this platform-only material.
use windows::{
    Foundation::{IPropertyValue, PropertyValue},
    Graphics::Effects::{
        IGraphicsEffect, IGraphicsEffect_Impl, IGraphicsEffectSource, IGraphicsEffectSource_Impl,
    },
    System::{DispatcherQueue, DispatcherQueueController},
    UI::{
        Color,
        Composition::{
            CompositionEffectSourceParameter, CompositionRoundedRectangleGeometry, Compositor,
            ContainerVisual, Desktop::DesktopWindowTarget, SpriteVisual,
        },
    },
    Win32::{
        Foundation::{E_INVALIDARG, E_NOTIMPL, HWND},
        System::WinRT::{
            Composition::ICompositorDesktopInterop,
            CreateDispatcherQueueController, DQTAT_COM_STA, DQTYPE_THREAD_CURRENT,
            DispatcherQueueOptions,
            Graphics::Direct2D::{
                GRAPHICS_EFFECT_PROPERTY_MAPPING, IGraphicsEffectD2D1Interop,
                IGraphicsEffectD2D1Interop_Impl,
            },
            RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
        },
    },
    core::{Error, GUID, HSTRING, Interface, PCWSTR, Result, implement},
};
use windows_numerics::Vector2;

const BLUR_STANDARD_DEVIATION: f32 = 24.0;

// One context per Overlay, reused across recording sessions. Never create a
// second thread/message loop. Release visuals before closing their compositor.
pub(in crate::adapters::overlay::windows) struct Context {
    compositor: Compositor,
    queue: Option<DispatcherQueueController>,
    _apartment: Apartment,
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            RoUninitialize();
        }
    }
}
impl Context {
    pub(in crate::adapters::overlay::windows) fn new() -> Result<Self> {
        unsafe {
            RoInitialize(RO_INIT_SINGLETHREADED)?;
        }
        let apartment = Apartment;
        let queue = if DispatcherQueue::GetForCurrentThread().is_ok() {
            None
        } else {
            Some(unsafe {
                CreateDispatcherQueueController(DispatcherQueueOptions {
                    dwSize: std::mem::size_of::<DispatcherQueueOptions>() as u32,
                    threadType: DQTYPE_THREAD_CURRENT,
                    apartmentType: DQTAT_COM_STA,
                })?
            })
        };
        Ok(Self {
            compositor: Compositor::new()?,
            queue,
            _apartment: apartment,
        })
    }
}
impl Drop for Context {
    fn drop(&mut self) {
        // UI thread owns both objects. Queue shutdown drains asynchronously through
        // the existing OS dispatcher; never block a native/GPUI callback waiting.
        let _ = self.compositor.Close();
        if let Some(queue) = &self.queue {
            let _ = queue.ShutdownQueueAsync();
        }
    }
}

pub(super) struct Material {
    target: DesktopWindowTarget,
    root: ContainerVisual,
    blur: SpriteVisual,
    tint: SpriteVisual,
    geometry: CompositionRoundedRectangleGeometry,
}
impl Material {
    pub(super) fn new(
        context: &Context,
        hwnd: windows_sys::Win32::Foundation::HWND,
        alpha: u8,
    ) -> Result<Self> {
        let compositor = &context.compositor;
        let interop: ICompositorDesktopInterop = compositor.cast()?;
        let target = unsafe { interop.CreateDesktopWindowTarget(HWND(hwnd), true)? };
        let source = CompositionEffectSourceParameter::Create(&HSTRING::from("backdrop"))?;
        let description: IGraphicsEffect = GaussianBlur {
            source: source.cast()?,
        }
        .into();
        let factory = compositor.CreateEffectFactory(&description)?;
        let brush = factory.CreateBrush()?;
        brush.SetSourceParameter(
            &HSTRING::from("backdrop"),
            &compositor.CreateBackdropBrush()?,
        )?;
        let blur = compositor.CreateSpriteVisual()?;
        blur.SetBrush(&brush)?;
        let tint = compositor.CreateSpriteVisual()?;
        tint.SetBrush(&compositor.CreateColorBrushWithColor(Color {
            A: alpha,
            R: 255,
            G: 255,
            B: 255,
        })?)?;
        let geometry = compositor.CreateRoundedRectangleGeometry()?;
        let clip = compositor.CreateGeometricClipWithGeometry(&geometry)?;
        let root = compositor.CreateContainerVisual()?;
        root.SetClip(&clip)?;
        root.Children()?.InsertAtBottom(&blur)?;
        root.Children()?.InsertAtTop(&tint)?;
        target.SetRoot(&root)?;
        Ok(Self {
            target,
            root,
            blur,
            tint,
            geometry,
        })
    }
    pub(super) fn resize(&self, width: usize, height: usize, scale: f64) -> Result<()> {
        let size = Vector2 {
            X: width as f32,
            Y: height as f32,
        };
        let radius = super::super::CORNER_RADIUS * scale as f32;
        self.geometry.SetSize(size)?;
        self.geometry.SetCornerRadius(Vector2 {
            X: radius,
            Y: radius,
        })?;
        self.root.SetSize(size)?;
        self.blur.SetSize(size)?;
        self.tint.SetSize(size)?;
        Ok(())
    }
}
impl Drop for Material {
    fn drop(&mut self) {
        let _ = self
            .target
            .SetRoot(None::<&windows::UI::Composition::Visual>);
        let _ = self.target.Close();
    }
}

// Win2D-compatible description of the built-in D2D GaussianBlur effect; no
// Win2D binary, custom shader, screen capture or CPU blur is required.
#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
struct GaussianBlur {
    source: IGraphicsEffectSource,
}
impl IGraphicsEffectSource_Impl for GaussianBlur_Impl {}
impl IGraphicsEffect_Impl for GaussianBlur_Impl {
    fn Name(&self) -> Result<HSTRING> {
        Ok(HSTRING::new())
    }
    fn SetName(&self, _: &HSTRING) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }
}
impl IGraphicsEffectD2D1Interop_Impl for GaussianBlur_Impl {
    fn GetEffectId(&self) -> Result<GUID> {
        Ok(GUID::from_u128(0x1feb6d69_2fe6_4ac9_8c58_1d7f93e7a6a5))
    }
    fn GetNamedPropertyMapping(
        &self,
        _: &PCWSTR,
        _: *mut u32,
        _: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> Result<()> {
        Err(Error::from_hresult(E_NOTIMPL))
    }
    fn GetPropertyCount(&self) -> Result<u32> {
        Ok(3)
    }
    fn GetProperty(&self, index: u32) -> Result<IPropertyValue> {
        match index {
            0 => PropertyValue::CreateSingle(BLUR_STANDARD_DEVIATION)?.cast(),
            1 => PropertyValue::CreateUInt32(1)?.cast(), // balanced optimization
            2 => PropertyValue::CreateUInt32(1)?.cast(), // hard border: no dark edge sampling
            _ => Err(Error::from_hresult(E_INVALIDARG)),
        }
    }
    fn GetSourceCount(&self) -> Result<u32> {
        Ok(1)
    }
    fn GetSource(&self, index: u32) -> Result<IGraphicsEffectSource> {
        if index == 0 {
            Ok(self.source.clone())
        } else {
            Err(Error::from_hresult(E_INVALIDARG))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_clips_the_whole_visual_tree_with_dpi_scaled_rounded_geometry() {
        let context = Context::new().unwrap();
        let host = super::super::Backdrop::new(std::ptr::null_mut(), &context, 110).unwrap();
        for dpi in [96, 144, 168, 192] {
            let scale = dpi as f64 / 96.0;
            let width = (224.0 * scale) as usize;
            let height = (100.0 * scale) as usize;
            assert!(host.place(
                windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
                width,
                height,
                scale
            ));
            let clip: windows::UI::Composition::CompositionGeometricClip =
                host.1.root.Clip().unwrap().cast().unwrap();
            let geometry: CompositionRoundedRectangleGeometry =
                clip.Geometry().unwrap().cast().unwrap();
            assert_eq!(
                geometry.Size().unwrap(),
                Vector2 {
                    X: width as f32,
                    Y: height as f32
                }
            );
            let radius = (16.0 * scale) as f32;
            assert_eq!(
                geometry.CornerRadius().unwrap(),
                Vector2 {
                    X: radius,
                    Y: radius
                }
            );
            assert_eq!(host.1.root.Children().unwrap().Count().unwrap(), 2);
        }
        assert_eq!(
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(host.0) },
            0
        );
    }

    #[test]
    fn blur_description_exposes_the_real_d2d_properties_and_rejects_bad_indices() {
        let _context = Context::new().unwrap();
        let source = CompositionEffectSourceParameter::Create(&HSTRING::from("backdrop")).unwrap();
        let effect: IGraphicsEffectD2D1Interop = GaussianBlur {
            source: source.cast().unwrap(),
        }
        .into();
        unsafe {
            assert_eq!(effect.GetPropertyCount().unwrap(), 3);
            assert_eq!(effect.GetProperty(0).unwrap().GetSingle().unwrap(), 24.0);
            assert_eq!(effect.GetProperty(2).unwrap().GetUInt32().unwrap(), 1);
            assert!(effect.GetProperty(3).is_err());
            assert_eq!(effect.GetSourceCount().unwrap(), 1);
            assert!(effect.GetSource(1).is_err());
        }
    }

    #[test]
    fn material_uses_requested_tint_alpha_without_scaling_foreground_or_geometry() {
        let context = Context::new().unwrap();
        for alpha in [1, 110, 255] {
            let host = super::super::Backdrop::new(std::ptr::null_mut(), &context, alpha).unwrap();
            let tint: windows::UI::Composition::CompositionColorBrush =
                host.1.tint.Brush().unwrap().cast().unwrap();
            assert_eq!(
                tint.Color().unwrap(),
                Color {
                    A: alpha,
                    R: 255,
                    G: 255,
                    B: 255
                }
            );
            assert_eq!(
                host.1.root.Opacity().unwrap(),
                1.0,
                "transparency belongs to tint, not foreground"
            );
        }
    }
}
