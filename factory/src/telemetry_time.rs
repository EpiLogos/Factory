//! Civil-day windows over Factory's owner-native timestamps. The governing
//! timezone and boundary come from Central's authored policy on this ground.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cli::CliError;

const MAX_DAYS: i64 = 366;

#[derive(Debug, Deserialize)]
struct CivilPolicy {
    schema: String,
    scope_ref: String,
    timezone: String,
    day_boundary_minutes: u32,
    #[serde(skip)]
    revision: String,
}

/// Inclusive named Days, represented as an exact half-open UTC interval.
#[derive(Debug, Clone)]
pub struct CivilWindow {
    pub from_day: NaiveDate,
    pub through_day: NaiveDate,
    pub timezone: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub policy_ref: String,
    pub policy_revision: String,
}

impl CivilWindow {
    pub fn from_args(
        args: &[String],
        state_path: &Path,
    ) -> Result<(Option<Self>, Vec<String>), CliError> {
        let mut day = None;
        let mut from = None;
        let mut through = None;
        let mut last = None;
        let mut remaining = Vec::new();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let slot = match arg.as_str() {
                "--day" => &mut day,
                "--from-day" => &mut from,
                "--through-day" => &mut through,
                "--last-days" => &mut last,
                _ => {
                    remaining.push(arg.clone());
                    continue;
                }
            };
            if slot.is_some() {
                return Err(CliError::new(format!("{arg} repeated")));
            }
            *slot = Some(
                iter.next()
                    .ok_or_else(|| CliError::new(format!("{arg} requires a value")))?
                    .clone(),
            );
        }
        let modes = usize::from(day.is_some())
            + usize::from(from.is_some() || through.is_some())
            + usize::from(last.is_some());
        if modes > 1 {
            return Err(CliError::new(
                "choose one civil window: --day, --from-day/--through-day, or --last-days",
            ));
        }
        if modes == 0 {
            return Ok((None, remaining));
        }
        let (policy, path) = read_policy(state_path)?;
        let window = if let Some(value) = day {
            let date = parse_day(&value)?;
            Self::build(date, date, &policy, &path)?
        } else if let Some(value) = last {
            let count: i64 = value
                .parse()
                .map_err(|_| CliError::new("--last-days requires an integer from 1 to 366"))?;
            if !(1..=MAX_DAYS).contains(&count) {
                return Err(CliError::new(
                    "--last-days requires an integer from 1 to 366",
                ));
            }
            let now: DateTime<Utc> = SystemTime::now().into();
            let tz = policy_timezone(&policy)?;
            let today = named_day_at(now, tz, policy.day_boundary_minutes)?;
            Self::build(today - Duration::days(count - 1), today, &policy, &path)?
        } else {
            let first = parse_day(
                &from.ok_or_else(|| CliError::new("--from-day requires --through-day"))?,
            )?;
            let final_day = parse_day(
                &through.ok_or_else(|| CliError::new("--through-day requires --from-day"))?,
            )?;
            Self::build(first, final_day, &policy, &path)?
        };
        Ok((Some(window), remaining))
    }

    pub fn for_day(state_path: &Path, day: NaiveDate) -> Result<Self, CliError> {
        let (policy, path) = read_policy(state_path)?;
        Self::build(day, day, &policy, &path)
    }

    pub fn for_range(
        state_path: &Path,
        from: NaiveDate,
        through: NaiveDate,
    ) -> Result<Self, CliError> {
        let (policy, path) = read_policy(state_path)?;
        Self::build(from, through, &policy, &path)
    }

    pub fn last_days(state_path: &Path, count: usize) -> Result<Self, CliError> {
        let args = vec!["--last-days".to_string(), count.to_string()];
        Ok(Self::from_args(&args, state_path)?
            .0
            .expect("window requested"))
    }

    pub fn previous(&self, state_path: &Path) -> Result<Self, CliError> {
        let days = (self.through_day - self.from_day).num_days() + 1;
        let through = self
            .from_day
            .pred_opt()
            .ok_or_else(|| CliError::new("previous Day overflows calendar"))?;
        Self::for_range(state_path, through - Duration::days(days - 1), through)
    }

    fn build(
        from: NaiveDate,
        through: NaiveDate,
        policy: &CivilPolicy,
        path: &Path,
    ) -> Result<Self, CliError> {
        let days = (through - from).num_days() + 1;
        if !(1..=MAX_DAYS).contains(&days) {
            return Err(CliError::new("civil window must cover 1 to 366 Days"));
        }
        let tz = policy_timezone(policy)?;
        let start_ms = boundary(from, tz, policy.day_boundary_minutes)?;
        let end_ms = boundary(
            through
                .succ_opt()
                .ok_or_else(|| CliError::new("last Day overflows calendar"))?,
            tz,
            policy.day_boundary_minutes,
        )?;
        Ok(Self {
            from_day: from,
            through_day: through,
            timezone: policy.timezone.clone(),
            start_ms,
            end_ms,
            policy_ref: path.display().to_string(),
            policy_revision: policy.revision.clone(),
        })
    }

    pub fn contains_ms(&self, instant: i64) -> bool {
        self.start_ms <= instant && instant < self.end_ms
    }

    pub fn overlaps_ms(&self, start: i64, end: i64) -> bool {
        start < end && start < self.end_ms && end > self.start_ms
    }

    pub fn contains_rfc3339(&self, value: &str) -> Option<bool> {
        parse_ms(value).map(|instant| self.contains_ms(instant))
    }

    pub fn as_json(&self) -> Value {
        json!({
            "fromDay": self.from_day.to_string(), "throughDay": self.through_day.to_string(),
            "timezone": self.timezone, "startUtc": DateTime::<Utc>::from_timestamp_millis(self.start_ms).map(|d| d.to_rfc3339()),
            "endUtcExclusive": DateTime::<Utc>::from_timestamp_millis(self.end_ms).map(|d| d.to_rfc3339()),
            "policyRef": self.policy_ref, "policyRevision": self.policy_revision,
            "interval": "half-open",
        })
    }
}

pub fn parse_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|t| t.timestamp_millis())
}

fn parse_day(value: &str) -> Result<NaiveDate, CliError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| CliError::new(format!("invalid civil Day `{value}`; expected YYYY-MM-DD")))
}

fn policy_timezone(policy: &CivilPolicy) -> Result<Tz, CliError> {
    Tz::from_str(&policy.timezone).map_err(|_| {
        CliError::new(format!(
            "Central policy has an unknown IANA timezone `{}`",
            policy.timezone
        ))
    })
}

fn named_day_at(
    instant: DateTime<Utc>,
    tz: Tz,
    boundary_minutes: u32,
) -> Result<NaiveDate, CliError> {
    if boundary_minutes >= 1440 {
        return Err(CliError::new(
            "Central civil policy day boundary is outside the Day",
        ));
    }
    Ok(
        (instant.with_timezone(&tz).naive_local() - Duration::minutes(i64::from(boundary_minutes)))
            .date(),
    )
}

fn boundary(day: NaiveDate, tz: Tz, minutes: u32) -> Result<i64, CliError> {
    if minutes >= 1440 {
        return Err(CliError::new(
            "Central civil policy day boundary is outside the Day",
        ));
    }
    let local = day
        .and_hms_opt(minutes / 60, minutes % 60, 0)
        .expect("validated minutes");
    tz.from_local_datetime(&local)
        .single()
        .map(|t| t.timestamp_millis())
        .ok_or_else(|| {
            CliError::new(format!(
                "civil boundary {local} is ambiguous or absent in {tz}"
            ))
        })
}

fn read_policy(state_path: &Path) -> Result<(CivilPolicy, PathBuf), CliError> {
    let absolute = if state_path.is_absolute() {
        state_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| CliError::new(e.to_string()))?
            .join(state_path)
    };
    let root = absolute
        .ancestors()
        .find(|p| p.join("Control").is_dir() && p.join("Work").is_dir())
        .ok_or_else(|| {
            CliError::new("civil Day window requires a Central ground above the state path")
        })?;
    let path = root.join("Control/user/civil-time-policy.json");
    let text = std::fs::read_to_string(&path).map_err(|e| {
        CliError::new(format!(
            "cannot read Central civil-time policy {}: {e}",
            path.display()
        ))
    })?;
    let mut policy: CivilPolicy = serde_json::from_str(&text)
        .map_err(|e| CliError::new(format!("invalid Central civil-time policy: {e}")))?;
    if policy.schema != "central.civil-time-policy/v1" || policy.scope_ref != "control:root" {
        return Err(CliError::new(
            "Central civil-time policy has an unsupported schema or scope",
        ));
    }
    policy.revision = format!("blake3:{}", blake3::hash(text.as_bytes()).to_hex());
    Ok((policy, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("Control/user")).unwrap();
        std::fs::create_dir_all(temp.path().join("Work")).unwrap();
        std::fs::write(temp.path().join("Control/user/civil-time-policy.json"),
            r#"{"schema":"central.civil-time-policy/v1","scope_ref":"control:root","timezone":"Europe/London","day_boundary_minutes":0,"automatic_day_rollover":true}"#).unwrap();
        let state = temp.path().join("Work/state.json");
        (temp, state)
    }

    #[test]
    fn london_days_use_actual_dst_boundaries() {
        let (_temp, state) = ground();
        let spring =
            CivilWindow::for_day(&state, NaiveDate::from_ymd_opt(2026, 3, 29).unwrap()).unwrap();
        let autumn =
            CivilWindow::for_day(&state, NaiveDate::from_ymd_opt(2026, 10, 25).unwrap()).unwrap();
        assert_eq!(spring.end_ms - spring.start_ms, 23 * 60 * 60 * 1000);
        assert_eq!(autumn.end_ms - autumn.start_ms, 25 * 60 * 60 * 1000);
        assert!(spring.contains_rfc3339("2026-03-29T00:30:00Z").unwrap());
        assert!(!spring.contains_rfc3339("2026-03-29T23:30:00Z").unwrap());
    }

    #[test]
    fn named_range_is_half_open_and_preserves_other_flags() {
        let (_temp, state) = ground();
        let args = vec![
            "--from-day",
            "2026-03-28",
            "--through-day",
            "2026-03-29",
            "--template",
            "attempts-by-agency",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        let (window, rest) = CivilWindow::from_args(&args, &state).unwrap();
        let window = window.unwrap();
        assert_eq!(rest, ["--template", "attempts-by-agency"]);
        assert!(window.contains_rfc3339("2026-03-28T23:30:00Z").unwrap());
        assert!(!window.contains_rfc3339("2026-03-29T23:00:00Z").unwrap());
        assert!(window.overlaps_ms(
            parse_ms("2026-03-27T12:00:00Z").unwrap(),
            parse_ms("2026-03-28T12:00:00Z").unwrap()
        ));
    }

    #[test]
    fn last_days_uses_governing_boundary_and_reports_exact_policy_revision() {
        let (temp, state) = ground();
        let policy_path = temp.path().join("Control/user/civil-time-policy.json");
        let mut policy: Value =
            serde_json::from_slice(&std::fs::read(&policy_path).unwrap()).unwrap();
        policy["day_boundary_minutes"] = json!(240);
        let bytes = serde_json::to_vec(&policy).unwrap();
        std::fs::write(&policy_path, &bytes).unwrap();
        let before_boundary = DateTime::parse_from_rfc3339("2026-03-29T02:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let tz: Tz = "Europe/London".parse().unwrap();
        assert_eq!(
            named_day_at(before_boundary, tz, 240).unwrap(),
            NaiveDate::from_ymd_opt(2026, 3, 28).unwrap()
        );
        let window =
            CivilWindow::for_day(&state, NaiveDate::from_ymd_opt(2026, 3, 28).unwrap()).unwrap();
        assert!(window.contains_ms(before_boundary.timestamp_millis()));
        assert_eq!(
            window.policy_revision,
            format!("blake3:{}", blake3::hash(&bytes).to_hex())
        );
        assert_eq!(window.as_json()["policyRevision"], window.policy_revision);
        policy["automatic_day_rollover"] = json!(false);
        std::fs::write(&policy_path, serde_json::to_vec(&policy).unwrap()).unwrap();
        let updated = CivilWindow::for_day(&state, window.from_day).unwrap();
        assert_ne!(updated.policy_revision, window.policy_revision);
    }
}
