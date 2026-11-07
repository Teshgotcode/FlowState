/**
 * Whether a post-process provider's base_url points off this machine. The
 * external-send notice fires only when this is true: local hosts and other
 * schemes (Apple Intelligence never leaves the process) stay silent, while
 * an empty or unparseable URL fails safe and counts as external.
 */
// Bun keeps IPv6 brackets in hostname, Node strips them: match both.
const LOCAL_HOSTS = ["localhost", "127.0.0.1", "::1", "[::1]"];

export const isExternalHost = (baseUrl: string): boolean => {
  try {
    const url = new URL(baseUrl);
    if (url.protocol !== "http:" && url.protocol !== "https:") return false;
    return !LOCAL_HOSTS.includes(url.hostname);
  } catch {
    return true;
  }
};
