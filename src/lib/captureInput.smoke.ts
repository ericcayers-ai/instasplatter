import { classifyCapturePaths } from "./captureInput.ts";

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(message);
}

const video = classifyCapturePaths(["C:\\capture\\walk.mp4"]);
assert(video.kind === "video", "single video requires confirmation");

const images = classifyCapturePaths([
  "C:\\capture\\frame_001.jpg",
  "C:\\capture\\frame_002.png",
]);
assert(images.kind === "images", "multiple image files form one capture");

const folder = classifyCapturePaths(["C:\\capture"]);
assert(folder.kind === "folder", "extensionless path is treated as a folder");

const mixed = classifyCapturePaths(["C:\\capture\\walk.mp4", "C:\\capture\\frame.jpg"]);
assert(mixed.kind === "unsupported", "mixed video/image selection is rejected");

console.log("captureInput.smoke.ts: ok");
