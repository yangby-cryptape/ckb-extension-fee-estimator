use std::{convert::TryInto, fmt, time::Duration};

use faketime::unix_time_as_millis;
use time::OffsetDateTime;

use crate::utilities::{PrettyDisplay, PrettyDisplayNewType};

impl PrettyDisplay for Duration {}

impl fmt::Display for PrettyDisplayNewType<Duration> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let format = "%0Y-%0m-%0d %0H:%0M:%0S %z";
        let dt =
            OffsetDateTime::from_unix_timestamp_nanos(self.as_ref().as_nanos().try_into().unwrap());
        write!(f, "{}", dt.format(format))
    }
}

pub(crate) fn unix_timestamp() -> Duration {
    Duration::from_millis(unix_time_as_millis())
}
