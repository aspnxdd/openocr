use std::borrow::Cow;

use freya::prelude::*;

pub struct TextDisplayWindow {
    pub text: Cow<'static, str>,
    pub img_bytes: Option<Bytes>,
}

impl App for TextDisplayWindow {
    fn render(&self) -> impl IntoElement {
        let t = self.text.clone();
        let maybe_img = self.img_bytes.clone().map(|bytes| {
            rect()
                .width(Size::fill())
                .center()
                .child(ImageViewer::new(("image", bytes)))
        });
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
