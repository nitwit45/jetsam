export const PAGES = ["overview", "clean", "aged", "memory", "freshen", "settings"] as const;
export type Page = (typeof PAGES)[number];
export const isPage = (s: string): s is Page => (PAGES as readonly string[]).includes(s);
