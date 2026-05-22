use crate::core::Point;
use crate::core::mouse::Button;
use crate::core::time::{Duration, Instant};

/// A mouse click.
#[derive(Debug, Clone, Copy)]
pub struct Click {
    kind: Kind,
    button: Button,
    position: Point,
    time: Instant,
    click_interval: Duration,
}

/// The kind of mouse click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A single click
    Single,

    /// A double click
    Double,

    /// A triple click
    Triple,
}

impl Kind {
    fn next(self) -> Kind {
        match self {
            Kind::Single => Kind::Double,
            Kind::Double => Kind::Triple,
            Kind::Triple => Kind::Double,
        }
    }
}

impl Click {
    /// Creates a new [`Click`] with the given position and previous last
    /// [`Click`].
    pub fn new(
        position: Point,
        button: Button,
        previous: Option<Click>,
        click_interval: Option<Duration>,
    ) -> Click {
        let time = Instant::now();

        let kind = if let Some(previous) = previous {
            if previous.is_consecutive(position, time)
                && button == previous.button
            {
                previous.kind.next()
            } else {
                Kind::Single
            }
        } else {
            Kind::Single
        };

        Click {
            kind,
            button,
            position,
            time,
            click_interval: click_interval
                .unwrap_or(Duration::from_millis(300)),
        }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    fn is_consecutive(&self, new_position: Point, time: Instant) -> bool {
        let duration = if time > self.time {
            Some(time - self.time)
        } else {
            None
        };

        self.position.distance(new_position) < 6.0
            && duration
                .map(|duration| duration <= self.click_interval)
                .unwrap_or(false)
    }
}
