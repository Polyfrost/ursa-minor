// Ursa Minor - A Hypixel API proxy
// Copyright (C) 2023 Linnea Gräf
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use chrono::Utc;
use hyper::http::request::Builder;
use hyper::Uri;
#[cfg(feature = "influxdb")]
use influxdb::Timestamp;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter, Write};
use std::ops::{Add, Deref, DerefMut, Sub};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

// Sadly need to use Url for url encoding, since hypers uri does not have that capability
pub trait UrlForRequest {
    fn url(self, url: Url) -> anyhow::Result<Self>
    where
        Self: Sized;
}

impl UrlForRequest for Builder {
    fn url(self, url: Url) -> anyhow::Result<Self> {
        Ok(self.uri(Uri::from_str(url.as_str())?))
    }
}

pub struct Obscure<T, const L: &'static str = "..">(pub T);

impl<T, const L: &'static str> Debug for Obscure<T, L> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(L)
    }
}

impl<T, const L: &'static str> Deref for Obscure<T, L> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, const L: &'static str> DerefMut for Obscure<T, L> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct MillisecondTimestamp(pub u64);

impl Debug for MillisecondTimestamp {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("https://time.is/{}", self.0))
    }
}

#[cfg(feature = "influxdb")]
impl From<MillisecondTimestamp> for Timestamp {
    fn from(value: MillisecondTimestamp) -> Self {
        chrono::DateTime::<Utc>::from(SystemTime::from(value)).into()
    }
}

#[cfg(feature = "influxdb")]
impl From<Timestamp> for MillisecondTimestamp {
    fn from(value: Timestamp) -> Self {
        let datetime: chrono::DateTime<Utc> = value.into();
        SystemTime::from(datetime).into()
    }
}
impl Add<Duration> for MillisecondTimestamp {
    type Output = MillisecondTimestamp;

    fn add(self, rhs: Duration) -> Self::Output {
        MillisecondTimestamp(self.0 + rhs.as_millis() as u64)
    }
}

impl Sub<MillisecondTimestamp> for MillisecondTimestamp {
    type Output = Duration;

    fn sub(self, rhs: MillisecondTimestamp) -> Self::Output {
        Duration::from_millis(self.0 - rhs.0)
    }
}

impl From<MillisecondTimestamp> for SystemTime {
    fn from(value: MillisecondTimestamp) -> Self {
        UNIX_EPOCH + Duration::from_millis(value.0)
    }
}
impl From<SystemTime> for MillisecondTimestamp {
    fn from(value: SystemTime) -> Self {
        // Fails in ~580 billion years
        MillisecondTimestamp(value.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64)
    }
}

impl MillisecondTimestamp {
    pub fn wait_time_or_zero(&self) -> Duration {
        let now = Self::now().unwrap();
        Duration::from_millis(self.0.saturating_sub(now.0))
    }

    pub fn now() -> anyhow::Result<Self> {
        Ok(MillisecondTimestamp::from(SystemTime::now()))
    }
}
pub fn pure_false() -> bool {
    false
}
