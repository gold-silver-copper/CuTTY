//! `parley_term` is the extraction target for CuTTY's terminal scene builder.
//!
//! The library owns cell-to-scene translation and leaves windowing, event
//! processing, terminal emulation, and app-specific overlays to adapters.

pub mod color;
pub mod font;
pub mod grid;
pub mod rects;
pub mod renderer;
pub mod scene;
pub mod text;

pub use color::{Rgb, color_from_rgb};
pub use font::{Font, FontDescription, FontOffset, FontSize};
pub use grid::{CellFlags, CursorShape, SceneCursor, SizeInfo, TerminalCell, TerminalGrid};
pub use rects::{RenderLine, RenderLines, RenderRect};
pub use renderer::{Error as RendererError, SceneRenderer};
pub use scene::{SceneBuilder, SceneFrame};
pub use text::{TextMetrics, TextSystem};
