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
        self.color(Color::Rgb(16, 18, 20))
    }

    pub(crate) fn surface(self) -> Color {
        self.color(Color::Rgb(21, 23, 25))
    }

    pub(crate) fn surface_high(self) -> Color {
        self.color(Color::Rgb(43, 45, 48))
    }

    pub(crate) fn border(self) -> Color {
        self.color(Color::Rgb(43, 45, 48))
    }

    pub(crate) fn text(self) -> Color {
        self.color(Color::Rgb(215, 219, 224))
    }

    pub(crate) fn bright_text(self) -> Color {
        self.color(Color::Rgb(240, 242, 244))
    }

    pub(crate) fn muted(self) -> Color {
        self.color(Color::Rgb(111, 118, 126))
    }

    pub(crate) fn accent(self) -> Color {
        self.color(Color::Rgb(142, 164, 199))
    }

    pub(crate) fn safe(self) -> Color {
        self.color(Color::Rgb(131, 169, 140))
    }

    pub(crate) fn review(self) -> Color {
        self.color(Color::Rgb(213, 178, 110))
    }

    pub(crate) fn protected(self) -> Color {
        self.color(Color::Rgb(212, 119, 119))
    }

    pub(crate) fn base(self) -> Style {
        Style::default().fg(self.text()).bg(self.background())
    }

    pub(crate) fn panel(self) -> Style {
        Style::default().fg(self.text()).bg(self.surface())
    }

    pub(crate) fn selected(self) -> Style {
        Style::default()
            .fg(self.bright_text())
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
