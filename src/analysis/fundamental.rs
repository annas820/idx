//! Plain-language valuation verdict. A handful of transparent rules; every
//! rule that fires is reported as a reason, so the reader can disagree.

use crate::model::{Fundamentals, Verdict, VerdictLabel};

/// Scores the ratios and writes the verdict into `f.verdict`.
pub fn verdict(f: &Fundamentals) -> Verdict {
    let mut score = 0;
    let mut reasons = Vec::new();
    let mut inputs = 0;
    let bank = f.is_financial_sector();
    let v = &f.valuation;
    let p = &f.profitability;

    if let Some(pe) = v.pe_trailing {
        inputs += 1;
        if pe <= 0.0 {
            score -= 1;
            reasons.push("negative trailing earnings".into());
        } else if pe < 10.0 {
            score += 2;
            reasons.push(format!("trailing P/E {pe:.1} is low (under 10)"));
        } else if pe < 15.0 {
            score += 1;
            reasons.push(format!("trailing P/E {pe:.1} is moderate (10 to 15)"));
        } else if pe > 35.0 {
            score -= 2;
            reasons.push(format!("trailing P/E {pe:.1} is high (over 35)"));
        } else if pe > 25.0 {
            score -= 1;
            reasons.push(format!("trailing P/E {pe:.1} is elevated (over 25)"));
        } else {
            reasons.push(format!("trailing P/E {pe:.1} is in a normal range"));
        }
    }

    if let Some(pb) = v.price_to_book {
        inputs += 1;
        if pb < 1.0 {
            score += 1;
            reasons.push(format!("trades below book value (P/B {pb:.2})"));
        } else if pb > 4.0 {
            score -= 1;
            reasons.push(format!("P/B {pb:.2} is rich (over 4)"));
        }
    }

    if let Some(peg) = v.peg {
        inputs += 1;
        if peg > 0.0 && peg < 1.0 {
            score += 1;
            reasons.push(format!("PEG {peg:.2} under 1: growth is cheap"));
        } else if peg > 2.0 {
            score -= 1;
            reasons.push(format!("PEG {peg:.2} over 2: paying up for growth"));
        }
    }

    if let Some(roe) = p.roe_pct {
        inputs += 1;
        if roe > 15.0 {
            score += 1;
            reasons.push(format!("ROE {roe:.1}% is strong (over 15%)"));
        } else if roe < 5.0 {
            score -= 1;
            reasons.push(format!("ROE {roe:.1}% is weak (under 5%)"));
        }
    }

    if let Some(nm) = p.net_margin_pct {
        inputs += 1;
        if nm > 20.0 {
            score += 1;
            reasons.push(format!("net margin {nm:.1}% is wide"));
        } else if nm < 5.0 {
            score -= 1;
            reasons.push(format!("net margin {nm:.1}% is thin"));
        }
    }

    if !bank {
        if let Some(de) = f.health.debt_to_equity {
            inputs += 1;
            if de > 2.0 {
                score -= 1;
                reasons.push(format!("debt is {de:.1}x equity: heavily leveraged"));
            } else if de < 0.5 {
                score += 1;
                reasons.push(format!(
                    "debt is {de:.2}x equity: conservative balance sheet"
                ));
            }
        }
    } else {
        reasons.push("leverage and margin rules relaxed for a financial company".into());
    }

    if let Some(g) = f.growth.revenue_growth_pct {
        inputs += 1;
        if g > 10.0 {
            score += 1;
            reasons.push(format!("revenue growing {g:.1}% year on year"));
        } else if g < 0.0 {
            score -= 1;
            reasons.push(format!("revenue shrinking {:.1}% year on year", -g));
        }
    }

    if let Some(g) = f.growth.earnings_growth_pct {
        inputs += 1;
        if g > 10.0 {
            score += 1;
            reasons.push(format!("earnings growing {g:.1}% year on year"));
        } else if g < 0.0 {
            score -= 1;
            reasons.push(format!("earnings shrinking {:.1}% year on year", -g));
        }
    }

    if let Some(y) = f.dividend.yield_pct {
        inputs += 1;
        if y >= 4.0 {
            score += 1;
            reasons.push(format!("dividend yield {y:.1}% is generous"));
        }
    }

    if let (Some(price), Some(target)) = (f.price, f.analyst.target_mean) {
        if price > 0.0 {
            let upside = (target - price) / price * 100.0;
            if upside > 20.0 {
                score += 1;
                reasons.push(format!("analyst mean target implies {upside:.0}% upside"));
            } else if upside < -10.0 {
                score -= 1;
                reasons.push(format!(
                    "analyst mean target sits {:.0}% below the price",
                    -upside
                ));
            }
        }
    }

    let label = if inputs < 3 {
        VerdictLabel::InsufficientData
    } else if score >= 3 {
        VerdictLabel::Undervalued
    } else if score <= -3 {
        VerdictLabel::Overvalued
    } else {
        VerdictLabel::Fair
    };
    Verdict {
        label,
        score,
        reasons,
        inputs_available: inputs,
    }
}

/// One-paragraph summary for the table footer.
pub fn summary_line(f: &Fundamentals) -> String {
    let Some(v) = &f.verdict else {
        return String::new();
    };
    let head = match v.label {
        VerdictLabel::Undervalued => "Looks undervalued on these ratios",
        VerdictLabel::Fair => "Looks fairly valued on these ratios",
        VerdictLabel::Overvalued => "Looks expensive on these ratios",
        VerdictLabel::InsufficientData => "Not enough data for a reading",
    };
    format!(
        "{head} (score {:+}, {} inputs). Heuristic from public ratios, not investment advice.",
        v.score, v.inputs_available
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Symbol;

    fn base() -> Fundamentals {
        Fundamentals::empty(Symbol::parse("TEST").unwrap(), "t")
    }

    #[test]
    fn cheap_quality_company_is_undervalued() {
        let mut f = base();
        f.valuation.pe_trailing = Some(8.0);
        f.valuation.price_to_book = Some(0.8);
        f.profitability.roe_pct = Some(18.0);
        f.dividend.yield_pct = Some(5.0);
        let v = verdict(&f);
        assert_eq!(v.label, VerdictLabel::Undervalued);
        assert!(v.score >= 3);
        assert!(v.reasons.iter().any(|r| r.contains("below book")));
    }

    #[test]
    fn expensive_company_is_overvalued() {
        let mut f = base();
        f.valuation.pe_trailing = Some(50.0);
        f.valuation.price_to_book = Some(9.0);
        f.valuation.peg = Some(3.0);
        f.growth.revenue_growth_pct = Some(-5.0);
        assert_eq!(verdict(&f).label, VerdictLabel::Overvalued);
    }

    #[test]
    fn too_little_data() {
        let mut f = base();
        f.valuation.pe_trailing = Some(5.0);
        let v = verdict(&f);
        assert_eq!(v.label, VerdictLabel::InsufficientData);
        assert_eq!(v.inputs_available, 1);
    }

    #[test]
    fn banks_skip_leverage_rule() {
        let mut f = base();
        f.sector = Some("Financial Services".into());
        f.health.debt_to_equity = Some(9.0);
        f.valuation.pe_trailing = Some(12.0);
        f.valuation.price_to_book = Some(2.0);
        f.profitability.roe_pct = Some(20.0);
        let v = verdict(&f);
        assert!(!v.reasons.iter().any(|r| r.contains("leveraged")));
        assert_eq!(v.label, VerdictLabel::Fair);
    }
}
