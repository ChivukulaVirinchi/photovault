import { convertFileSrc } from "@tauri-apps/api/core";
import { library } from "./api/library";
import { photos } from "./api/photos";

const MAX_POSTER_EDGE = 860;
const VIDEO_METADATA_TIMEOUT_MS = 8_000;
const VIDEO_SEEK_TIMEOUT_MS = 5_000;

export async function probeVideoPoster(id: number): Promise<string | null> {
  const { absolute_path } = await library.resolvePath(id);
  const src = convertFileSrc(absolute_path);
  const video = document.createElement("video");
  video.preload = "metadata";
  video.muted = true;
  video.playsInline = true;
  video.crossOrigin = "anonymous";

  try {
    await loadMetadata(video, src);
    const durationMs = Number.isFinite(video.duration)
      ? Math.round(video.duration * 1000)
      : null;
    const width = video.videoWidth || null;
    const height = video.videoHeight || null;
    const posterBase64 = await capturePoster(video);
    const result = await photos.saveVideoProbe({
      id,
      duration_ms: durationMs,
      width,
      height,
      poster_jpeg_base64: posterBase64,
    });
    return result.thumbnail_path;
  } finally {
    video.removeAttribute("src");
    video.load();
  }
}

function loadMetadata(video: HTMLVideoElement, src: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      cleanup();
      reject(new Error("video metadata decode timed out"));
    }, VIDEO_METADATA_TIMEOUT_MS);
    const cleanup = () => {
      window.clearTimeout(timer);
      video.onloadedmetadata = null;
      video.onerror = null;
    };
    video.onloadedmetadata = () => {
      cleanup();
      resolve();
    };
    video.onerror = () => {
      cleanup();
      reject(new Error("video metadata decode failed"));
    };
    video.src = src;
  });
}

async function capturePoster(video: HTMLVideoElement): Promise<string | null> {
  if (!video.videoWidth || !video.videoHeight) return null;
  const target = Number.isFinite(video.duration)
    ? Math.min(1, Math.max(0, video.duration * 0.1))
    : 0;
  await seek(video, target);

  const scale = Math.min(1, MAX_POSTER_EDGE / Math.max(video.videoWidth, video.videoHeight));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(video.videoWidth * scale));
  canvas.height = Math.max(1, Math.round(video.videoHeight * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(video, 0, 0, canvas.width, canvas.height);
  const blob = await new Promise<Blob | null>((resolve) =>
    canvas.toBlob(resolve, "image/jpeg", 0.82),
  );
  if (!blob) return null;
  return blobToBase64(blob);
}

function seek(video: HTMLVideoElement, seconds: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => {
      cleanup();
      reject(new Error("video seek timed out"));
    }, VIDEO_SEEK_TIMEOUT_MS);
    const cleanup = () => {
      window.clearTimeout(timer);
      video.onseeked = null;
      video.onerror = null;
    };
    video.onseeked = () => {
      cleanup();
      resolve();
    };
    video.onerror = () => {
      cleanup();
      reject(new Error("video seek failed"));
    };
    video.currentTime = seconds;
  });
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const value = String(reader.result ?? "");
      resolve(value.includes(",") ? value.split(",", 2)[1] : value);
    };
    reader.onerror = () => reject(reader.error ?? new Error("blob read failed"));
    reader.readAsDataURL(blob);
  });
}
