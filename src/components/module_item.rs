use crate::{components::position_button, theme::use_theme};
use iced::{Alignment, Element, Length, widget::container};

use super::ButtonUIRef;

/// Builder for a bar module item: content wrapped in a position_button
/// with optional press, right-press, middle-press, scroll-up, and scroll-down handlers.
///
/// When no press handler is set, renders as a plain container.
///
/// # The module-slot contract
///
/// This is the one place every module's `view()` output passes through
/// before reaching the bar (`modules::get_module_view` -> `build_module_item`
/// -> here). Both branches below give the content a real `height(Fill)` plus
/// `align_y(Center)`, matching how COSMIC's `Context::button_from_element`
/// and GNOME Shell's `PanelMenu.Button` (`St.Bin` with `y_align: CENTER`)
/// force arbitrary applet/indicator content into one shared, vertically
/// centered slot -- centering is structural here, not something each
/// module is expected to remember.
///
/// That said, `height(Fill)` only has real space to center *within* if its
/// ancestor chain actually provides a fixed height instead of `Shrink`
/// (see `modules_section` in `src/modules/mod.rs`), and it only centers
/// this element as one block -- it does NOT fix mismatched alignment
/// *inside* a module's own content. If a module's `view()` builds a
/// `row!`/`Row` mixing children of different intrinsic heights (an icon
/// glyph next to text is the classic case: icon and text line-heights
/// differ), that row must set its own `.align_y(Alignment::Center)` --
/// otherwise it defaults to top-alignment and looks pinned to the top of
/// the bar regardless of this wrapper. This exact bug hit `SystemInfo`'s
/// `indicator_info_element` and `custom_module`'s icon+text row; when
/// adding a new module, check every `row!`/`Row` that combines an icon
/// with text for a missing `align_y`.
pub struct ModuleItem<'a, Msg> {
    content: Element<'a, Msg>,
    on_press: Option<Msg>,
    on_press_with_position: Option<Box<dyn Fn(ButtonUIRef) -> Msg + 'a>>,
    on_right_press: Option<Msg>,
    on_middle_press: Option<Msg>,
    on_scroll_up: Option<Msg>,
    on_scroll_down: Option<Msg>,
}

pub fn module_item<'a, Msg: 'static + Clone>(content: Element<'a, Msg>) -> ModuleItem<'a, Msg> {
    ModuleItem {
        content,
        on_press: None,
        on_press_with_position: None,
        on_right_press: None,
        on_middle_press: None,
        on_scroll_up: None,
        on_scroll_down: None,
    }
}

impl<'a, Msg: 'static + Clone> ModuleItem<'a, Msg> {
    pub fn on_press(mut self, msg: Msg) -> Self {
        self.on_press = Some(msg);
        self
    }

    pub fn on_press_with_position(mut self, handler: impl Fn(ButtonUIRef) -> Msg + 'a) -> Self {
        self.on_press_with_position = Some(Box::new(handler));
        self
    }

    pub fn on_right_press(mut self, msg: Msg) -> Self {
        self.on_right_press = Some(msg);
        self
    }

    pub fn on_middle_press(mut self, msg: Msg) -> Self {
        self.on_middle_press = Some(msg);
        self
    }

    pub fn on_scroll_up(mut self, msg: Msg) -> Self {
        self.on_scroll_up = Some(msg);
        self
    }

    pub fn on_scroll_down(mut self, msg: Msg) -> Self {
        self.on_scroll_down = Some(msg);
        self
    }
}

impl<'a, Msg: 'static + Clone> From<ModuleItem<'a, Msg>> for Element<'a, Msg> {
    fn from(item: ModuleItem<'a, Msg>) -> Self {
        let (space, module_button_style) =
            use_theme(|theme| (theme.space, theme.module_button_style()));

        let has_action = item.on_press.is_some() || item.on_press_with_position.is_some();

        if has_action {
            let mut button = position_button(
                container(item.content)
                    .align_y(Alignment::Center)
                    .height(Length::Fill)
                    .clip(true),
            )
            .padding([0.0, space.xs])
            .height(Length::Fill)
            .style(module_button_style);

            if let Some(handler) = item.on_press_with_position {
                button = button.on_press_with_position(handler);
            } else if let Some(msg) = item.on_press {
                button = button.on_press(msg);
            }

            if let Some(msg) = item.on_right_press {
                button = button.on_right_press(msg);
            }
            if let Some(msg) = item.on_middle_press {
                button = button.on_middle_press(msg);
            }
            if let Some(msg) = item.on_scroll_up {
                button = button.on_scroll_up(msg);
            }
            if let Some(msg) = item.on_scroll_down {
                button = button.on_scroll_down(msg);
            }

            button.into()
        } else {
            container(item.content)
                .padding([0.0, space.xs])
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .clip(true)
                .into()
        }
    }
}
