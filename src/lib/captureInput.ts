const VIDEO_EXTENSIONS = new Set(["mp4", "mov", "avi", "mkv", "webm", "m4v", "mts", "3gp"]);
const IMAGE_EXTENSIONS = new Set(["jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff"]);

export type CapturePathKind = "video" | "images" | "folder" | "unsupported";

export interface ClassifiedCapturePaths {
  kind: CapturePathKind;
  paths: string[];
}

function extension(path: string): string {
  const name = path.replace(/\\/g, "/").split("/").pop() ?? "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function classifyCapturePaths(paths: string[]): ClassifiedCapturePaths {
  const clean = paths.map((path) => path.trim()).filter(Boolean);
  if (clean.length === 0) return { kind: "unsupported", paths: [] };

  const extensions = clean.map(extension);
  if (extensions.every((ext) => VIDEO_EXTENSIONS.has(ext))) {
    return { kind: "video", paths: clean };
  }
  if (extensions.every((ext) => IMAGE_EXTENSIONS.has(ext))) {
    return { kind: "images", paths: clean };
  }
  if (clean.length === 1 && extensions[0] === "") {
    return { kind: "folder", paths: clean };
  }
  return { kind: "unsupported", paths: clean };
}

export const CAPTURE_VIDEO_EXTENSIONS = [...VIDEO_EXTENSIONS];
export const CAPTURE_IMAGE_EXTENSIONS = [...IMAGE_EXTENSIONS];
