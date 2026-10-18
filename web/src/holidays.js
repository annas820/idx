// IDX trading holidays: the web twin of src/holidays.rs. holidays.json is
// data/holidays.csv as [date, name, name_en] rows (a test keeps them equal);
// weekend holidays are not listed, as IDX publishes them.

import HOLIDAYS from "./holidays.json" with { type: "json" };

const BY_DATE = new Map(HOLIDAYS.map(([date, name, nameEn]) => [date, { name, name_en: nameEn || null }]));
const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

const dow = (iso) => new Date(`${iso}T00:00:00Z`).getUTCDay();
const addDays = (iso, n) => new Date(Date.parse(`${iso}T00:00:00Z`) + n * 86400e3).toISOString().slice(0, 10);

export const weekday = (iso) => WEEKDAYS[dow(iso)];

/** Years the list covers, oldest first. */
export const years = () => [...new Set(HOLIDAYS.map(([d]) => Number(d.slice(0, 4))))].sort((a, b) => a - b);
export const covers = (year) => years().includes(year);

/** A weekday that is not a listed holiday (for an uncovered year only weekends close). */
export const isTradingDay = (iso) => dow(iso) !== 0 && dow(iso) !== 6 && !BY_DATE.has(iso);

export function nextTradingDay(iso) {
  let d = addDays(iso, 1);
  while (!isTradingDay(d)) d = addDays(d, 1);
  return d;
}

/** Weekdays minus listed holidays in `year`. */
export function tradingDays(year) {
  let n = 0;
  for (let d = `${year}-01-01`; d.startsWith(String(year)); d = addDays(d, 1)) if (isTradingDay(d)) n++;
  return n;
}

/** The idx.holidays.v1 payload for `year` (every year when null) plus warnings, like service::holidays. */
export function holidays(year, upcoming, today) {
  const covered = years();
  const warnings = [];
  if (year != null && !covers(year)) warnings.push(`no holiday list for ${year}; covered: ${covered.join(", ")}`);
  const list = HOLIDAYS
    .filter(([d]) => year == null || Number(d.slice(0, 4)) === year)
    .filter(([d]) => !upcoming || d >= today)
    .map(([date, name, nameEn]) => ({ date, weekday: weekday(date), name, name_en: nameEn || null }));
  return {
    data: {
      year: year ?? null,
      years_covered: covered,
      trading_days: year != null && covers(year) ? tradingDays(year) : null,
      today,
      today_is_trading_day: isTradingDay(today),
      next_trading_day: nextTradingDay(today),
      holidays: list,
    },
    warnings,
  };
}
