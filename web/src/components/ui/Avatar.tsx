/**
 * Avatar — circular initial badge on a user-derived color.
 *
 * Matches the visual language of the WorkspaceHeader avatar and the
 * presence bar. Color is derived deterministically from the seed (usually
 * the user_id) so the same user always shows the same hue.
 */

type Size = "sm" | "md";

const sizes: Record<Size, string> = {
  sm: "h-5 w-5 text-[10px]",
  md: "h-6 w-6 text-[11px]",
};

export function Avatar({
  name,
  seed,
  size = "sm",
  title,
}: {
  name: string;
  seed: string;
  size?: Size;
  title?: string;
}) {
  return (
    <span
      aria-hidden
      title={title ?? name}
      className={`inline-flex items-center justify-center rounded-full text-white font-semibold select-none shrink-0 ${sizes[size]}`}
      style={{ background: colorFor(seed) }}
    >
      {(name || "?").slice(0, 1).toUpperCase()}
    </span>
  );
}

export function colorFor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  return `hsl(${hash % 360}, 70%, 45%)`;
}
