//! Cached native images; only the GUI thread changes the status item.
use super::status_icon::{self, Animation, Frame, Mode};
use anyhow::Result;
use serde_json::Value;
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const INITIAL_FRAME: Frame = Frame {
    mode: Mode::Idle,
    level: 0,
};

pub(super) struct Mark {
    images: Vec<Icon>,
    animation: Animation,
}
impl Mark {
    pub fn new() -> Result<Self> {
        let images = status_icon::frames()
            .map(|frame| {
                Icon::from_rgba(
                    status_icon::rgba(frame, cfg!(target_os = "macos")),
                    status_icon::PIXELS as u32,
                    status_icon::PIXELS as u32,
                )
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(Self {
            images,
            animation: Animation::seeded(INITIAL_FRAME),
        })
    }
    pub fn build(&self, builder: TrayIconBuilder) -> TrayIconBuilder {
        let image = self.images[INITIAL_FRAME.cache_index()].clone();
        #[cfg(target_os = "macos")]
        {
            builder.with_icon_templated(image)
        }
        #[cfg(not(target_os = "macos"))]
        {
            builder.with_icon(image)
        }
    }
    pub fn update(
        &mut self,
        icon: &TrayIcon,
        runtime: &Value,
        now: u64,
        reduced_motion: bool,
    ) -> Result<()> {
        if let Some(frame) = self.animation.update(runtime, now, reduced_motion) {
            let image = Some(self.images[frame.cache_index()].clone());
            #[cfg(target_os = "macos")]
            {
                icon.set_icon_templated(image)?;
                super::menu_header::size_mark(icon);
            }
            #[cfg(not(target_os = "macos"))]
            icon.set_icon(image)?;
        }
        Ok(())
    }
}
