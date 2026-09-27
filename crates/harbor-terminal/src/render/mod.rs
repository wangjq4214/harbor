pub mod background;
pub mod cursor;
pub use crate::cursor_blink;
pub mod decoration;
pub mod gpu;
pub use crate::layout;
pub mod pipeline;
pub mod scrollbar;
pub mod selection;
pub mod text;

pub use background::Background;
pub use cursor::Cursor;
pub use cursor_blink::CursorBlinkState;
pub use decoration::Decoration;
pub use gpu::{
    TerminalGpuAccess, UploadMode, UploadPlan, UploadPolicy, alpha_mode_supports_transparency,
};
pub use layout::{PreeditGlyph, PreeditLayout, RenderViewport, layout_preedit};
pub use pipeline::TerminalRenderPipeline;
pub use scrollbar::{Scrollbar, ScrollbarHit, hit_test, offset_for_thumb};
pub use selection::Selection;
pub use text::Text;
