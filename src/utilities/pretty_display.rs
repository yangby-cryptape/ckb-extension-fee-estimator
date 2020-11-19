use std::fmt;

pub(crate) struct PrettyDisplayNewType<T>(T);

pub(crate) trait PrettyDisplay
where
    Self: Sized,
    PrettyDisplayNewType<Self>: fmt::Display,
{
    fn pretty(self) -> PrettyDisplayNewType<Self> {
        PrettyDisplayNewType(self)
    }
}

impl<T> AsRef<T> for PrettyDisplayNewType<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}
