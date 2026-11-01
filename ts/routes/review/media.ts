// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Plays a card's [sound:] tags in the page, as the card's frame can't.
//
// Phones won't start playback without a user gesture. Android Chrome remembers one
// for the page, while iOS wants each media element to have been played from inside
// one, after which scripts can change its source and play it freely. So there's one
// audio element for the life of the page, and unlockMedia() blesses it at the first
// tap. If playback is still refused, the reviewer offers a button.

import type { AVTag } from "@generated/anki/card_rendering_pb";

import { mediaUrl } from "./lib";

/** All that's played. Others are refused, even if a browser could play their audio. */
const AUDIO_EXTENSIONS = new Set(["aac", "flac", "m4a", "mp3", "oga", "ogg", "opus", "wav"]);

/** A single silent sample. */
const SILENCE = "data:audio/wav;base64,UklGRiYAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQIAAACAgA==";

export function isAudioFile(filename: string): boolean {
    return AUDIO_EXTENSIONS.has(filename.split(".").pop()?.toLowerCase() ?? "");
}

let audio: HTMLAudioElement | null = null;
let unlocked = false;

function audioElement(): HTMLAudioElement {
    return (audio ??= document.createElement("audio"));
}

/** Call from a click or key press, before anything needs to play. */
export function unlockMedia(): void {
    const element = audioElement();
    // something that's already playing can stay that way, and is as good
    if (!unlocked && element.paused) {
        element.src = SILENCE;
        element.play().then(() => (unlocked = true)).catch(() => undefined);
    }
}

export interface PlayerCallbacks {
    /** The browser refused to play without a gesture, or has since allowed it. */
    onBlocked(blocked: boolean): void;
    /** This isn't an audio file, so it was skipped. */
    onNotAudio(filename: string): void;
}

export class MediaPlayer {
    private queue: AVTag[] = [];
    private pending: AVTag[] = [];
    /** Stops events of earlier items from advancing the queue. */
    private generation = 0;
    private removeListeners: (() => void)[] = [];

    constructor(private frameBase: string, private callbacks: PlayerCallbacks) {}

    /** Puts the element in the page, under `container`. */
    mount(container: HTMLElement): void {
        const audio = audioElement();
        audio.hidden = true;
        container.append(audio);
        const next = () => this.playNext();
        audio.addEventListener("ended", next);
        audio.addEventListener("error", next);
        this.removeListeners.push(() => {
            audio.removeEventListener("ended", next);
            audio.removeEventListener("error", next);
        });
    }

    unmount(): void {
        this.stop();
        for (const remove of this.removeListeners) {
            remove();
        }
        this.removeListeners = [];
        audio?.remove();
    }

    /** Replaces what's playing. */
    play(tags: AVTag[]): void {
        this.stop();
        this.queue = [...tags];
        this.playNext();
    }

    stop(): void {
        this.generation += 1;
        this.queue = [];
        this.pending = [];
        audio?.pause();
        this.callbacks.onBlocked(false);
    }

    /** From a button press: tries again what the browser refused. */
    resume(): void {
        unlockMedia();
        const tags = this.pending;
        this.play(tags);
    }

    private playNext(): void {
        const tag = this.queue.shift();
        if (!tag) {
            return;
        }
        if (isAudioFile(tag.sound)) {
            this.playFile(tag);
        } else {
            this.callbacks.onNotAudio(tag.sound);
            this.playNext();
        }
    }

    private refused(tag: AVTag): void {
        this.pending = [tag, ...this.queue];
        this.queue = [];
        this.callbacks.onBlocked(true);
    }

    private playFile(tag: AVTag): void {
        const element = audioElement();
        element.src = mediaUrl(this.frameBase, tag.sound);
        const generation = this.generation;
        element.play().then(
            () => {
                unlocked = true;
                if (generation === this.generation) {
                    this.callbacks.onBlocked(false);
                }
            },
            (err) => {
                if (err.name === "NotAllowedError" && generation === this.generation) {
                    this.refused(tag);
                } else if (err.name !== "AbortError") {
                    // A file the browser can't play gets an error event as well.
                    console.debug(err);
                }
            },
        );
    }
}
