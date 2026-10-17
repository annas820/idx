//! Indicator maths over daily candles. Pure functions, unit tested with
//! hand-checked sequences. `analyse` assembles the `Technical` report.

use chrono::Duration;

use crate::model::{
    Bollinger, Candle, History, Levels, Macd, MovingAverages, Signal, SignalLabel, Technical,
    Trend, VolumeStats,
};

/// Simple moving average of the last `n` values.
pub fn sma(values: &[f64], n: usize) -> Option<f64> {
    if n == 0 || values.len() < n {
        return None;
    }
    let tail = &values[values.len() - n..];
    Some(tail.iter().sum::<f64>() / n as f64)
}

/// Exponential moving average series (standard `2/(n+1)` smoothing, seeded
/// with the SMA of the first `n` values). Returns one value per input from
/// index `n-1` onwards.
pub fn ema_series(values: &[f64], n: usize) -> Vec<f64> {
    if n == 0 || values.len() < n {
        return vec![];
    }
    let k = 2.0 / (n as f64 + 1.0);
    let mut out = Vec::with_capacity(values.len() - n + 1);
    let mut prev = values[..n].iter().sum::<f64>() / n as f64;
    out.push(prev);
    for v in &values[n..] {
        prev = v * k + prev * (1.0 - k);
        out.push(prev);
    }
    out
}

pub fn ema(values: &[f64], n: usize) -> Option<f64> {
    ema_series(values, n).last().copied()
}

/// Wilder's RSI over `n` periods.
pub fn rsi(closes: &[f64], n: usize) -> Option<f64> {
    if n == 0 || closes.len() < n + 1 {
        return None;
    }
    let mut gain = 0.0;
    let mut loss = 0.0;
    for w in closes[..=n].windows(2) {
        let d = w[1] - w[0];
        if d > 0.0 {
            gain += d;
        } else {
            loss -= d;
        }
    }
    let mut avg_gain = gain / n as f64;
    let mut avg_loss = loss / n as f64;
    for w in closes[n..].windows(2) {
        let d = w[1] - w[0];
        let (g, l) = if d > 0.0 { (d, 0.0) } else { (0.0, -d) };
        avg_gain = (avg_gain * (n as f64 - 1.0) + g) / n as f64;
        avg_loss = (avg_loss * (n as f64 - 1.0) + l) / n as f64;
    }
    if avg_loss == 0.0 {
        return Some(100.0);
    }
    let rs = avg_gain / avg_loss;
    Some(100.0 - 100.0 / (1.0 + rs))
}

/// MACD(fast, slow, signal). Returns the latest line, signal and histogram.
pub fn macd(closes: &[f64], fast: usize, slow: usize, signal: usize) -> Option<Macd> {
    if closes.len() < slow + signal {
        return None;
    }
    let ef = ema_series(closes, fast);
    let es = ema_series(closes, slow);
    // Align: `es` starts `slow - fast` entries later than `ef`.
    let offset = slow - fast;
    let line: Vec<f64> = es
        .iter()
        .enumerate()
        .map(|(i, s)| ef[i + offset] - s)
        .collect();
    let sig = ema_series(&line, signal);
    let l = *line.last()?;
    let s = *sig.last()?;
    Some(Macd {
        line: l,
        signal: s,
        histogram: l - s,
    })
}

/// Bollinger bands with population standard deviation.
pub fn bollinger(closes: &[f64], n: usize, k: f64) -> Option<Bollinger> {
    let mid = sma(closes, n)?;
    let tail = &closes[closes.len() - n..];
    let var = tail.iter().map(|c| (c - mid).powi(2)).sum::<f64>() / n as f64;
    let sd = var.sqrt();
    let upper = mid + k * sd;
    let lower = mid - k * sd;
    let close = *closes.last()?;
    let width = upper - lower;
    Some(Bollinger {
        upper,
        middle: mid,
        lower,
        percent_b: if width > 0.0 {
            (close - lower) / width
        } else {
            0.5
        },
        bandwidth_pct: if mid != 0.0 { width / mid * 100.0 } else { 0.0 },
    })
}

/// Wilder's average true range.
pub fn atr(candles: &[Candle], n: usize) -> Option<f64> {
    if n == 0 || candles.len() < n + 1 {
        return None;
    }
    let tr: Vec<f64> = candles
        .windows(2)
        .map(|w| {
            let (p, c) = (&w[0], &w[1]);
            (c.high - c.low)
                .max((c.high - p.close).abs())
                .max((c.low - p.close).abs())
        })
        .collect();
    let mut a = tr[..n].iter().sum::<f64>() / n as f64;
    for t in &tr[n..] {
        a = (a * (n as f64 - 1.0) + t) / n as f64;
    }
    Some(a)
}

/// Swing highs/lows: a bar whose high (low) is the extreme of the `w` bars on
/// each side. Looks at the last `lookback` bars. Returns up to three support
/// levels below the price (nearest first) and three resistance levels above.
pub fn levels(candles: &[Candle], lookback: usize, w: usize, price: f64) -> Levels {
    let start = candles.len().saturating_sub(lookback);
    let slice = &candles[start..];
    let mut support = Vec::new();
    let mut resistance = Vec::new();
    if slice.len() < 2 * w + 1 {
        return Levels {
            support,
            resistance,
        };
    }
    for i in w..slice.len() - w {
        let c = &slice[i];
        let window = &slice[i - w..=i + w];
        let others = window.iter().filter(|o| !std::ptr::eq(*o, c));
        if others.clone().all(|o| c.high > o.high) && c.high > price {
            resistance.push(c.high);
        }
        if others.clone().all(|o| c.low < o.low) && c.low < price {
            support.push(c.low);
        }
    }
    dedupe_levels(&mut support, price);
    dedupe_levels(&mut resistance, price);
    support.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    resistance.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    support.truncate(3);
    resistance.truncate(3);
    Levels {
        support,
        resistance,
    }
}

/// Collapses levels within 0.5 % of each other.
fn dedupe_levels(v: &mut Vec<f64>, price: f64) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let tol = price.abs() * 0.005;
    let mut out: Vec<f64> = Vec::new();
    for &x in v.iter() {
        match out.last() {
            Some(&last) if (x - last).abs() <= tol => {}
            _ => out.push(x),
        }
    }
    *v = out;
}

fn pct_change(now: f64, then: f64) -> Option<f64> {
    if then == 0.0 {
        None
    } else {
        Some((now - then) / then * 100.0)
    }
}

/// Close `days` calendar days before the last candle (or the nearest earlier bar).
fn close_days_ago(candles: &[Candle], days: i64) -> Option<f64> {
    let last = candles.last()?;
    let target = last.time - Duration::days(days);
    candles
        .iter()
        .rev()
        .find(|c| c.time <= target)
        .map(|c| c.close)
}

pub fn trend(price: f64, ma: &MovingAverages, sma50_prev: Option<f64>) -> Trend {
    match (ma.sma50, ma.sma200) {
        (Some(s50), Some(s200)) => {
            let rising = sma50_prev.map(|p| s50 >= p).unwrap_or(true);
            if price > s50 && s50 > s200 && rising {
                Trend::Uptrend
            } else if price < s50 && s50 < s200 && !rising {
                Trend::Downtrend
            } else {
                Trend::Sideways
            }
        }
        (Some(s50), None) => {
            let rising = sma50_prev.map(|p| s50 >= p).unwrap_or(true);
            if price > s50 * 1.02 && rising {
                Trend::Uptrend
            } else if price < s50 * 0.98 && !rising {
                Trend::Downtrend
            } else {
                Trend::Sideways
            }
        }
        _ => Trend::Unknown,
    }
}

/// Builds the report from daily candles (oldest first).
pub fn analyse(h: &History) -> Option<Technical> {
    let candles = &h.candles;
    let last = candles.last()?;
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let price = last.close;

    let ma = MovingAverages {
        sma20: sma(&closes, 20),
        sma50: sma(&closes, 50),
        sma200: sma(&closes, 200),
        ema20: ema(&closes, 20),
        ema50: ema(&closes, 50),
        ema200: ema(&closes, 200),
    };
    let sma50_prev = if closes.len() > 55 {
        sma(&closes[..closes.len() - 5], 50)
    } else {
        None
    };
    let rsi14 = rsi(&closes, 14);
    let macd_now = macd(&closes, 12, 26, 9);
    let macd_prev = if closes.len() > 1 {
        macd(&closes[..closes.len() - 1], 12, 26, 9)
    } else {
        None
    };
    let boll = bollinger(&closes, 20, 2.0);
    let atr14 = atr(candles, 14);

    let vols: Vec<f64> = candles.iter().map(|c| c.volume as f64).collect();
    let avg20 = sma(&vols, 20);
    let volume = VolumeStats {
        last: Some(last.volume),
        avg20: avg20.map(|v| v.round() as u64),
        ratio: avg20.filter(|a| *a > 0.0).map(|a| last.volume as f64 / a),
    };

    let year_ago = last.time - Duration::days(365);
    let last_year: Vec<&Candle> = candles.iter().filter(|c| c.time >= year_ago).collect();
    let high_52w = last_year
        .iter()
        .map(|c| c.high)
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.max(x))));
    let low_52w = last_year
        .iter()
        .map(|c| c.low)
        .fold(None, |m: Option<f64>, x| Some(m.map_or(x, |m| m.min(x))));

    let lv = levels(candles, 120, 5, price);
    let tr = trend(price, &ma, sma50_prev);

    // Golden / death cross within the last 10 bars.
    let mut cross: Option<&str> = None;
    if closes.len() >= 210 {
        let now = (sma(&closes, 50)?, sma(&closes, 200)?);
        let before = &closes[..closes.len() - 10];
        let then = (sma(before, 50)?, sma(before, 200)?);
        if now.0 > now.1 && then.0 <= then.1 {
            cross = Some("golden");
        } else if now.0 < now.1 && then.0 >= then.1 {
            cross = Some("death");
        }
    }

    let signal = signal(
        price,
        &ma,
        rsi14,
        macd_now.as_ref(),
        macd_prev.as_ref(),
        boll.as_ref(),
        tr,
        cross,
        &volume,
    );

    Some(Technical {
        symbol: h.symbol.clone(),
        name: h.name.clone(),
        currency: h.currency.clone(),
        as_of: last.time,
        period: h.period,
        candles_used: candles.len(),
        price,
        change_pct_1d: candles
            .len()
            .checked_sub(2)
            .and_then(|i| pct_change(price, candles[i].close)),
        change_pct_1w: close_days_ago(candles, 7).and_then(|c| pct_change(price, c)),
        change_pct_1m: close_days_ago(candles, 30).and_then(|c| pct_change(price, c)),
        change_pct_3m: close_days_ago(candles, 91).and_then(|c| pct_change(price, c)),
        high_52w,
        low_52w,
        moving_averages: ma,
        rsi14,
        macd: macd_now,
        bollinger: boll,
        atr14,
        volume,
        levels: lv,
        trend: tr,
        signal,
    })
}

#[allow(clippy::too_many_arguments)]
fn signal(
    price: f64,
    ma: &MovingAverages,
    rsi14: Option<f64>,
    macd_now: Option<&Macd>,
    macd_prev: Option<&Macd>,
    boll: Option<&Bollinger>,
    tr: Trend,
    cross: Option<&str>,
    volume: &VolumeStats,
) -> Signal {
    let mut score = 0;
    let mut reasons = Vec::new();

    match tr {
        Trend::Uptrend => {
            score += 1;
            reasons.push(
                "price above a rising 50-day average that sits above the 200-day: uptrend".into(),
            );
        }
        Trend::Downtrend => {
            score -= 1;
            reasons.push(
                "price below a falling 50-day average that sits below the 200-day: downtrend"
                    .into(),
            );
        }
        Trend::Sideways => reasons.push("moving averages disagree: no clear trend".into()),
        Trend::Unknown => reasons.push("not enough history for the 50/200-day trend read".into()),
    }

    if let Some(r) = rsi14 {
        if r < 30.0 {
            score += 1;
            reasons.push(format!("RSI {r:.0} is oversold (<30)"));
        } else if r > 70.0 {
            score -= 1;
            reasons.push(format!("RSI {r:.0} is overbought (>70)"));
        } else {
            reasons.push(format!("RSI {r:.0} is neutral"));
        }
    }

    if let (Some(now), Some(prev)) = (macd_now, macd_prev) {
        if now.histogram > 0.0 && prev.histogram <= 0.0 {
            score += 1;
            reasons.push("MACD just crossed above its signal line".into());
        } else if now.histogram < 0.0 && prev.histogram >= 0.0 {
            score -= 1;
            reasons.push("MACD just crossed below its signal line".into());
        } else if now.histogram > 0.0 {
            score += 1;
            reasons.push("MACD above its signal line (bullish momentum)".into());
        } else if now.histogram < 0.0 {
            score -= 1;
            reasons.push("MACD below its signal line (bearish momentum)".into());
        }
    }

    if let Some(s200) = ma.sma200 {
        if price > s200 {
            score += 1;
            reasons.push("price above the 200-day average".into());
        } else {
            score -= 1;
            reasons.push("price below the 200-day average".into());
        }
    }

    match cross {
        Some("golden") => {
            score += 1;
            reasons
                .push("golden cross in the last 10 sessions (50-day crossed above 200-day)".into());
        }
        Some("death") => {
            score -= 1;
            reasons
                .push("death cross in the last 10 sessions (50-day crossed below 200-day)".into());
        }
        _ => {}
    }

    if let Some(b) = boll {
        if b.percent_b < 0.0 {
            score += 1;
            reasons.push("close below the lower Bollinger band (stretched to the downside)".into());
        } else if b.percent_b > 1.0 {
            score -= 1;
            reasons.push("close above the upper Bollinger band (stretched to the upside)".into());
        }
    }

    if let Some(ratio) = volume.ratio {
        if ratio >= 2.0 {
            reasons.push(format!(
                "volume {ratio:.1}x its 20-day average: the move has participation"
            ));
        }
    }

    let label = if score >= 2 {
        SignalLabel::Buy
    } else if score <= -2 {
        SignalLabel::Sell
    } else {
        SignalLabel::Neutral
    };
    Signal {
        label,
        score,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{wib_from_unix, Interval, Period, Symbol};

    fn candles(closes: &[f64]) -> Vec<Candle> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Candle {
                time: wib_from_unix(1_700_000_000 + i as i64 * 86_400).unwrap(),
                open: c,
                high: c + 1.0,
                low: c - 1.0,
                close: c,
                adj_close: None,
                volume: 100,
            })
            .collect()
    }

    #[test]
    fn sma_and_ema_basics() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(sma(&v, 5), Some(3.0));
        assert_eq!(sma(&v, 2), Some(4.5));
        assert_eq!(sma(&v, 6), None);
        // EMA(3) seeded with SMA of first three = 2, k = 0.5: 3, 4
        let e = ema_series(&v, 3);
        assert_eq!(e, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn rsi_matches_reference_sequence() {
        // Classic textbook series (Wilder); RSI(14) after the 15th close ~ 70.53.
        let closes = [
            44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
            45.61, 46.28, 46.28,
        ];
        let r = rsi(&closes, 14).unwrap();
        assert!((r - 70.53).abs() < 0.1, "{r}");
        assert_eq!(rsi(&closes[..10], 14), None);
        // Monotonic rise -> 100.
        let up: Vec<f64> = (1..=20).map(|x| x as f64).collect();
        assert_eq!(rsi(&up, 14), Some(100.0));
    }

    #[test]
    fn macd_and_bollinger_shapes() {
        let closes: Vec<f64> = (1..=60).map(|x| 100.0 + (x as f64).sin() * 5.0).collect();
        let m = macd(&closes, 12, 26, 9).unwrap();
        assert!((m.histogram - (m.line - m.signal)).abs() < 1e-9);
        assert_eq!(macd(&closes[..30], 12, 26, 9), None);

        let flat = vec![10.0; 25];
        let b = bollinger(&flat, 20, 2.0).unwrap();
        assert_eq!(b.upper, 10.0);
        assert_eq!(b.percent_b, 0.5);
        let b = bollinger(&closes, 20, 2.0).unwrap();
        assert!(b.lower < b.middle && b.middle < b.upper);
    }

    #[test]
    fn atr_on_constant_range() {
        let c = candles(&[10.0; 20]);
        // high-low = 2 every bar, no gaps -> ATR 2.
        assert!((atr(&c, 14).unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn levels_find_swings_around_price() {
        // Valley at index 10 (low 89), peak at 30 (high 121), price ends at 100.
        let mut closes = vec![100.0; 45];
        closes[10] = 90.0;
        closes[30] = 120.0;
        let c = candles(&closes);
        let lv = levels(&c, 120, 5, 100.0);
        assert_eq!(lv.support, vec![89.0]);
        assert_eq!(lv.resistance, vec![121.0]);
    }

    #[test]
    fn analyse_full_report() {
        let closes: Vec<f64> = (0..260).map(|i| 1000.0 + i as f64 * 2.0).collect();
        let h = History {
            symbol: Symbol::parse("TEST").unwrap(),
            name: None,
            currency: Some("IDR".into()),
            period: Period::Y1,
            interval: Interval::Day1,
            candles: candles(&closes),
            summary: None,
        };
        let t = analyse(&h).unwrap();
        assert_eq!(t.candles_used, 260);
        assert!(t.moving_averages.sma200.is_some());
        assert_eq!(t.trend, Trend::Uptrend);
        assert_eq!(t.rsi14, Some(100.0));
        assert_eq!(t.signal.label, SignalLabel::Neutral, "{:?}", t.signal);
        assert!(t.change_pct_1d.unwrap() > 0.0);
        assert!(t.high_52w.unwrap() >= t.price);
        assert_eq!(t.volume.ratio, Some(1.0));

        // Too few candles: still a report, with nulls.
        let short = History {
            candles: candles(&closes[..10]),
            ..h
        };
        let t = analyse(&short).unwrap();
        assert_eq!(t.rsi14, None);
        assert_eq!(t.trend, Trend::Unknown);
        assert!(t.macd.is_none());
    }
}
