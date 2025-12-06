import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import {
  FALLBACK_LANGUAGE,
  SUPPORTED_LANGUAGES,
  SupportedLanguage,
  detectInitialLanguage,
  getLanguageLabelKey,
  storeLanguage,
  translate,
  type TranslationValues,
} from "@/lib/i18n";

type LanguageContextValue = {
  language: SupportedLanguage;
  setLanguage: (language: SupportedLanguage) => void;
};

const LanguageContext = createContext<LanguageContextValue | null>(null);

export const LanguageProvider = ({ children }: { children: React.ReactNode }) => {
  const [language, setLanguageState] = useState<SupportedLanguage>(() => detectInitialLanguage());

  useEffect(() => {
    document.documentElement.lang = language;
    storeLanguage(language);
  }, [language]);

  const setLanguage = useCallback((next: SupportedLanguage) => {
    if (!SUPPORTED_LANGUAGES.includes(next)) return;
    setLanguageState(next);
  }, []);

  const value = useMemo(() => ({ language, setLanguage }), [language, setLanguage]);

  return <LanguageContext.Provider value={value}>{children}</LanguageContext.Provider>;
};

export const useLanguage = () => {
  const ctx = useContext(LanguageContext);
  if (!ctx) {
    throw new Error("useLanguage must be used within a LanguageProvider");
  }
  return ctx;
};

export const LANGUAGE_LABELS: Record<SupportedLanguage, string> = {
  en: "English",
  ko: "한국어",
  ja: "日本語",
  zh: "中文",
};

export const useI18n = () => {
  const { language } = useLanguage();
  const t = useCallback(
    (key: string, values?: TranslationValues) => translate(language ?? FALLBACK_LANGUAGE, key, values),
    [language],
  );

  return { t, language, languageLabelKey: getLanguageLabelKey(language ?? FALLBACK_LANGUAGE) };
};

export const languageOptions: SupportedLanguage[] = ["en", "ko"];
