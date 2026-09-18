/**
 * The two formatters every panel that shows a run needs. They live here
 * rather than in each panel because a response and its history row must
 * round a size and bucket a status IDENTICALLY — the same run shown twice
 * with two different sizes reads as a bug in the app, not a rounding
 * difference.
 */

/** Response size, in the largest unit that keeps the number readable. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

/**
 * Which of the three semantic colors a status wears: 2xx reads as ok, 3xx as
 * a warning, everything else (4xx, 5xx, and the 0 an unsent step carries) as
 * a failure.
 */
export function statusClass(status: number): "ok" | "redirect" | "err" {
  const leading = Math.floor(status / 100);
  if (leading === 2) return "ok";
  if (leading === 3) return "redirect";
  return "err";
}
