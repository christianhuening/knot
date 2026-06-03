import { useEffect, useState } from "react";

export type Viewport = "mobile" | "tablet" | "desktop";

const MOBILE_MAX = 640;
const TABLET_MAX = 1024;

function classify(width: number): Viewport {
  if (width < MOBILE_MAX) return "mobile";
  if (width < TABLET_MAX) return "tablet";
  return "desktop";
}

/** Returns the current viewport bucket, recomputed on window resize. */
export function useViewport(): Viewport {
  const [vp, setVp] = useState<Viewport>(() =>
    typeof window === "undefined" ? "desktop" : classify(window.innerWidth),
  );
  useEffect(() => {
    const onResize = () => setVp(classify(window.innerWidth));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return vp;
}
