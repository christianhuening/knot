/** Read a cookie value by name, or null. */
export function readCookie(name: string): string | null {
  const re = new RegExp(`(?:^|; )${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}=([^;]*)`);
  const m = document.cookie.match(re);
  return m && m[1] ? decodeURIComponent(m[1]) : null;
}
