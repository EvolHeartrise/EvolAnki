// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

use crate::collection::Collection;
use crate::error::Result;
use crate::prelude::*;

pub mod answering;
pub(crate) mod filtered;
pub mod fsrs;
pub mod new;
pub(crate) mod queue;
mod service;
pub mod states;
pub mod suspend;
pub mod timespan;
pub mod timing;
mod upgrade;

use chrono::FixedOffset;
use timing::sched_timing_today;
use timing::SchedTimingToday;

#[derive(Debug, Clone, Copy)]
pub struct SchedulerInfo {
    pub timing: SchedTimingToday,
}

impl Collection {
    pub fn scheduler_info(&mut self) -> Result<SchedulerInfo> {
        let now = TimestampSecs::now();
        if let Some(mut info) = self.state.scheduler_info {
            if now < info.timing.next_day_at {
                info.timing.now = now;
                return Ok(info);
            }
        }
        let timing = self.timing_for_timestamp(now)?;
        let info = SchedulerInfo { timing };
        self.state.scheduler_info = Some(info);
        Ok(info)
    }

    pub fn timing_today(&mut self) -> Result<SchedTimingToday> {
        self.scheduler_info().map(|info| info.timing)
    }

    pub fn current_due_day(&mut self, delta: i32) -> Result<u32> {
        Ok(((self.timing_today()?.days_elapsed as i32) + delta).max(0) as u32)
    }

    pub(crate) fn timing_for_timestamp(&mut self, now: TimestampSecs) -> Result<SchedTimingToday> {
        let current_utc_offset = self.local_utc_offset_for_user()?;

        let rollover_hour = match self.get_v2_rollover() {
            Some(hour) => hour,
            None => {
                // an older Anki version failed to set this; correct the issue
                self.set_v2_rollover(4)?;
                4
            }
        };

        sched_timing_today(
            self.storage.creation_stamp()?,
            now,
            self.creation_utc_offset(),
            current_utc_offset,
            rollover_hour,
        )
    }

    /// Return the current local timezone offset, ensuring the config reflects
    /// the current value.
    pub(crate) fn local_utc_offset_for_user(&mut self) -> Result<FixedOffset> {
        let config_tz = self
            .get_configured_utc_offset()
            .and_then(|v| FixedOffset::west_opt(v * 60))
            .unwrap_or_else(|| FixedOffset::west_opt(0).unwrap());

        let local_tz = TimestampSecs::now().local_utc_offset()?;

        // if the timezone has changed, update the config
        if config_tz != local_tz {
            self.set_configured_utc_offset(local_tz.utc_minus_local() / 60)?;
        }
        Ok(local_tz)
    }

    /// Return the timezone offset at collection creation time. This should
    /// only be set when the V2 scheduler is active and the new timezone
    /// code is enabled.
    fn creation_utc_offset(&self) -> Option<FixedOffset> {
        self.get_creation_utc_offset()
            .and_then(|v| FixedOffset::west_opt(v * 60))
    }

    pub fn rollover_for_current_scheduler(&self) -> u8 {
        self.get_v2_rollover().unwrap_or(4)
    }
}
