use std::borrow::Cow;

use freya::prelude::*;

pub struct TextDisplayWindow {
    pub text: Cow<'static, str>,
    pub img_bytes: Option<Vec<u8>>,
}

impl App for TextDisplayWindow {
    fn render(&self) -> impl IntoElement {
        let t = self.text.clone();
        let img_bytes = self.img_bytes.clone();
        let maybe_img = if let Some(bytes) = &img_bytes {
            let src = ImageSource::Bytes(0, Bytes::from(bytes.clone()));
            Some(
                rect()
                    .width(Size::FillMinimum)
                    .center()
                    .child(ImageViewer::new(src).into_element()),
            )
        } else {
            None
        };
        rect()
            .padding(30.0)
            .spacing(10.0)
            .center()
            .maybe_child(maybe_img)
            .child(label().text(t.clone()))
            .child(
                Button::new()
                    .on_press(move |_| {
                        if let Err(e) = Clipboard::set(t.clone().into()) {
                            eprintln!("Failed to copy to clipboard: {:?}", e);
                        }
                    })
                    .outline()
                    .child(
                        rect()
                            .content(Content::Flex)
                            .horizontal()
                            .spacing(20.0)
                            .child(svg(freya::icons::lucide::copy()))
                            .child(label().text("Copy").color((0, 0, 0))),
                    ),
            )
    }
}
