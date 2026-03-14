use softbuffer::{Context, Surface};
use std::rc::Rc;
use winit::{monitor::MonitorHandle, window::Window};

use crate::selection::Selection;

pub struct OcrWindow {
    pub selection: Selection,
    pub monitor: MonitorHandle,
    #[allow(dead_code)]
    pub context: Context<Rc<Window>>,
    pub surface: Surface<Rc<Window>, Rc<Window>>,
    pub window: Rc<Window>,
}
