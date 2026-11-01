// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

export type Breakpoint = "xs" | "sm" | "md" | "lg" | "xl" | "xxl";

export type HelpItem = {
    title: string;
    help?: string;
    url?: string;
    global?: boolean;
};

export type IconData = {
    url: string;
};
