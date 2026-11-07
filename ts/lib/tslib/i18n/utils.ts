// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

const langs = ["en-US"];

export function weekdayLabel(n: number): string {
    const now = new Date();
    const daysFromToday = -now.getDay() + n;
    const desiredDay = new Date(now.getTime() + daysFromToday * 86_400_000);
    return desiredDay.toLocaleDateString(langs, {
        weekday: "narrow",
    });
}

export function localizedDate(
    date: Date,
    options?: Intl.DateTimeFormatOptions,
): string {
    return date.toLocaleDateString(langs, options);
}

export function localizedNumber(n: number, precision = 2): string {
    const round = Math.pow(10, precision);
    const rounded = Math.round(n * round) / round;
    return rounded.toLocaleString(langs);
}

export function createLocaleNumberFormat(options?: Intl.NumberFormatOptions): Intl.NumberFormat {
    return new Intl.NumberFormat(langs, options);
}
