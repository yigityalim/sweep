use ratatui::style::{Color, Modifier, Style};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Theme {
    no_color: bool,
}

impl Theme {
    pub(crate) const fn new(no_color: bool) -> Self {
        Self { no_color }
    }

    pub(crate) fn background(self) -> Color {
        self.color(Color::Rgb(7, 10, 15))
    }

    pub(crate) fn surface(self) -> Color {
        self.color(Color::Rgb(13, 18, 26))
    }

    pub(crate) fn surface_high(self) -> Color {
        self.color(Color::Rgb(24, 34, 47))
    }

    pub(crate) fn border(self) -> Color {
        self.color(Color::Rgb(48, 59, 74))
    }

    pub(crate) fn text(self) -> Color {
        self.color(Color::Rgb(224, 231, 239))
    }

    pub(crate) fn muted(self) -> Color {
        self.color(Color::Rgb(126, 140, 158))
    }

    pub(crate) fn accent(self) -> Color {
        self.color(Color::Rgb(91, 169, 255))
    }

    pub(crate) fn safe(self) -> Color {
        self.color(Color::Rgb(87, 210, 138))
    }

    pub(crate) fn review(self) -> Color {
        self.color(Color::Rgb(241, 190, 83))
    }

    pub(crate) fn protected(self) -> Color {
        self.color(Color::Rgb(244, 105, 105))
    }

    pub(crate) fn base(self) -> Style {
        Style::default().fg(self.text()).bg(self.background())
    }

    pub(crate) fn panel(self) -> Style {
        Style::default().fg(self.text()).bg(self.surface())
    }

    pub(crate) fn selected(self) -> Style {
        Style::default()
            .fg(self.text())
            .bg(self.surface_high())
            .add_modifier(Modifier::BOLD)
    }

    pub(crate) fn decision(self, decision: &str) -> Style {
        match decision {
            "safe" => Style::default().fg(self.safe()),
            "review" => Style::default().fg(self.review()),
            "protected" => Style::default().fg(self.protected()),
            _ => Style::default().fg(self.muted()),
        }
    }

    fn color(self, color: Color) -> Color {
        if self.no_color { Color::Reset } else { color }
    }
}
