use std::{convert::TryInto, fmt, time::Duration};

use faketime::unix_time_as_millis;
use time::{macros::format_description, OffsetDateTime};

use crate::utilities::{PrettyDisplay, PrettyDisplayNewType};

impl PrettyDisplay for Duration {}

impl fmt::Display for PrettyDisplayNewType<Duration> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let format = format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3] \
            [offset_hour sign:mandatory]:[offset_minute]"
        );
        let ts = self.as_ref().as_nanos().try_into().unwrap();
        let dt = OffsetDateTime::from_unix_timestamp_nanos(ts).unwrap();
        write!(f, "{}", dt.format(format).unwrap())
    }
}

pub(crate) fn unix_timestamp() -> Duration {
    Duration::from_millis(unix_time_as_millis())
}
