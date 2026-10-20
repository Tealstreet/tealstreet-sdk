//! Contract V3 rate-limit windows, shared by all reads on this egress.
use safe_cex_engine::Response;
pub fn delay(response: Response<'_>, now: f64) -> f64 {
    if let Some(value) = response.header("retry-after") {
        let ms = value
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
            .map(|n| n * 1000.0)
            .or_else(|| http_date(value).map(|time| time - now));
        if let Some(ms) = ms.filter(|n| *n > 0.0) {
            return ms.min(60_000.0);
        }
    }
    let (mut exhausted, mut observed): (f64, f64) = (0.0, 0.0);
    for line in response.headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_uppercase();
        let Some(suffix) = name.rsplit('-').next().filter(|s| s.len() > 1) else {
            continue;
        };
        let (number, unit) = suffix.split_at(suffix.len() - 1);
        let Some(number) = number
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n > 0.0)
        else {
            continue;
        };
        let unit = match unit {
            "S" => 1000.0,
            "M" => 60_000.0,
            "H" => 3_600_000.0,
            "D" => 86_400_000.0,
            _ => continue,
        };
        let ms = number * unit;
        if name.starts_with("X-REMAINING-WEIGHT-") || name.starts_with("X-ORDER-REMAINING-") {
            if value.trim().parse::<f64>().is_ok_and(|n| n <= 0.0) {
                exhausted = exhausted.max(ms);
            }
        } else if name.starts_with("X-USED-WEIGHT-") || name.starts_with("X-ORDER-COUNT-") {
            observed = observed.max(ms);
        }
    }
    if exhausted > 0.0 {
        exhausted.min(60_000.0)
    } else if observed > 0.0 {
        observed.min(60_000.0)
    } else {
        10_000.0
    }
}
fn http_date(value: &str) -> Option<f64> {
    let p: Vec<_> = value.split_ascii_whitespace().collect();
    if p.len() != 6 || p[5] != "GMT" {
        return None;
    }
    let day: i64 = p[1].parse().ok()?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == p[2])? as i64
        + 1;
    let year: i64 = p[3].parse().ok()?;
    let time: Vec<i64> = p[4]
        .split(':')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if !(1970..=9999).contains(&year)
        || !(1..=31).contains(&day)
        || time.len() != 3
        || !(0..24).contains(&time[0])
        || !(0..60).contains(&time[1])
        || !(0..60).contains(&time[2])
    {
        return None;
    }
    let days = safe_cex_engine::time::days_from_civil(year, month, day);
    Some(((days * 86400 + time[0] * 3600 + time[1] * 60 + time[2]) * 1000) as f64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retry_after_then_exhausted_then_observed_then_fallback() {
        let r = |headers| Response {
            status: 429,
            headers,
            body: "",
        };
        assert_eq!(
            delay(r("Retry-After: 3\nX-REMAINING-WEIGHT-1M: 0"), 0.0),
            3000.0
        );
        assert_eq!(
            delay(r("X-REMAINING-WEIGHT-10S: 0\nX-USED-WEIGHT-1M: 2000"), 0.0),
            10_000.0
        );
        assert_eq!(delay(r("X-ORDER-COUNT-1H: 50"), 0.0), 60_000.0);
        assert_eq!(delay(r(""), 0.0), 10_000.0);
        assert_eq!(
            delay(
                r("Retry-After: Sun, 06 Nov 1994 08:49:37 GMT"),
                784_111_772_000.0
            ),
            5000.0
        );
    }
}
