import { useEffect, useState } from "react";
import { resolveEffectiveTheme, type EffectiveTheme, type ThemePreference } from "./preferences";

const DARK_MEDIA_QUERY = "(prefers-color-scheme: dark)";

function mediaQuery(): MediaQueryList | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia(DARK_MEDIA_QUERY)
    : null;
}

function systemPrefersDark(): boolean {
  return mediaQuery()?.matches ?? false;
}

export function useEffectiveTheme(preference: ThemePreference): EffectiveTheme {
  const [effectiveTheme, setEffectiveTheme] = useState(() =>
    resolveEffectiveTheme(preference, systemPrefersDark()),
  );

  useEffect(() => {
    const query = mediaQuery();
    const update = () =>
      setEffectiveTheme(resolveEffectiveTheme(preference, query?.matches ?? false));
    update();
    query?.addEventListener("change", update);
    return () => query?.removeEventListener("change", update);
  }, [preference]);

  return effectiveTheme;
}
