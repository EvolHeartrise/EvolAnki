// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

// Plays a card's [sound:] tags in the page, as the card's frame can't.
//
// Phones won't start playback without a user gesture. Android Chrome remembers one
// for the page, while iOS wants each media element to have been played from inside
// one, after which scripts can change its source and play it freely. So there's one
// audio and one video element for the life of the page, and unlockMedia() blesses
// them at the first tap. If playback is still refused, the reviewer offers a button.

import type { AVTag } from "@generated/anki/card_rendering_pb";

import { mediaUrl } from "./lib";

/** What upstream Anki plays in a video player. */
const VIDEO_EXTENSIONS = new Set(["avi", "flv", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "ogv", "webm"]);

/** A single silent sample. */
const SILENCE = "data:audio/wav;base64,UklGRiYAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQIAAACAgA==";

export function isVideoFile(filename: string): boolean {
    return VIDEO_EXTENSIONS.has(filename.split(".").pop()?.toLowerCase() ?? "");
}

let audio: HTMLAudioElement | null = null;
let video: HTMLVideoElement | null = null;
const unlocked = new Set<HTMLMediaElement>();

function audioElement(): HTMLAudioElement {
    return (audio ??= document.createElement("audio"));
}

function videoElement(): HTMLVideoElement {
    if (!video) {
        video = document.createElement("video");
        video.playsInline = true;
        video.controls = true;
    }
    return video;
}

/** Call from a click or key press, before anything needs to play. */
export function unlockMedia(): void {
    for (const element of [audioElement(), videoElement()]) {
        // something that's already playing can stay that way, and is as good
        if (!unlocked.has(element) && element.paused) {
            element.src = SILENCE;
            element.play().then(() => unlocked.add(element)).catch(() => undefined);
        }
    }
}

export interface PlayerCallbacks {
    /** The browser refused to play without a gesture, or has since allowed it. */
    onBlocked(blocked: boolean): void;
    /** The video element should or shouldn't be shown. */
    onVideo(showing: boolean): void;
}

export class MediaPlayer {
    private queue: AVTag[] = [];
    private pending: AVTag[] = [];
    /** Stops events of earlier items from advancing the queue. */
    private generation = 0;
    private removeListeners: (() => void)[] = [];

    constructor(private frameBase: string, private callbacks: PlayerCallbacks) {}

    /** Puts the elements in the page, under `container`. */
    mount(container: HTMLElement): void {
        const audio = audioElement();
        const video = videoElement();
        audio.hidden = true;
        container.append(audio, video);
        for (const element of [audio, video]) {
            const next = () => this.playNext();
            element.addEventListener("ended", next);
            element.addEventListener("error", next);
            this.removeListeners.push(() => {
                element.removeEventListener("ended", next);
                element.removeEventListener("error", next);
            });
        }
    }

    unmount(): void {
        this.stop();
        for (const remove of this.removeListeners) {
            remove();
        }
        this.removeListeners = [];
        audio?.remove();
        video?.remove();
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
        video?.pause();
        this.callbacks.onVideo(false);
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
        if (tag.value.case === "soundOrVideo") {
            this.playFile(tag, tag.value.value);
        } else {
            this.playNext();
        }
    }

    private refused(tag: AVTag): void {
        this.pending = [tag, ...this.queue];
        this.queue = [];
        this.callbacks.onBlocked(true);
    }

    private playFile(tag: AVTag, filename: string): void {
        const isVideo = isVideoFile(filename);
        const element = isVideo ? videoElement() : audioElement();
        this.callbacks.onVideo(isVideo);
        element.src = mediaUrl(this.frameBase, filename);
        const generation = this.generation;
        element.play().then(
            () => {
                unlocked.add(element);
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
