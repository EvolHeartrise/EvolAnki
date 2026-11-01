// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

import { i18nResources } from "@generated/backend";
import type { ModuleName } from "@generated/ftl";
import { FluentBundle, FluentResource } from "@generated/ftl";
import { setBundles } from "@generated/ftl";

export function weekdayLabel(n: number): string {
    const now = new Date();
    const daysFromToday = -now.getDay() + n;
    const desiredDay = new Date(now.getTime() + daysFromToday * 86_400_000);
    return desiredDay.toLocaleDateString(langs, {
        weekday: "narrow",
    });
}

let langs: string[] = [];

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

export function localeCompare(
    first: string,
    second: string,
    options?: Intl.CollatorOptions,
): number {
    return first.localeCompare(second, langs, options);
}

/** Treat text like HTML, merging multiple spaces and converting
 newlines to spaces. */
export function withCollapsedWhitespace(s: string): string {
    return s.replace(/\s+/g, " ");
}

export function withoutUnicodeIsolation(s: string): string {
    return s.replace(/[\u2068-\u2069]+/g, "");
}

export async function setupI18n(args: { modules: ModuleName[] }): Promise<void> {
    const resources = await i18nResources(args);
    const json = JSON.parse(new TextDecoder().decode(resources.json));

    const newBundles: FluentBundle[] = [];
    for (const res in json.resources) {
        const text = json.resources[res];
        const lang = json.langs[res];
        const bundle = new FluentBundle([lang, "en-US"]);
        const resource = new FluentResource(text);
        bundle.addResource(resource);
        newBundles.push(bundle);
    }

    setBundles(newBundles);
    langs = json.langs;
}

let globalI18n: Promise<void> | undefined;

export async function setupGlobalI18n(): Promise<void> {
    if (!globalI18n) {
        globalI18n = setupI18n({
            modules: [],
        });
    }
    await globalI18n;
}
