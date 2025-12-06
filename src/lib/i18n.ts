import enCommon from "@/i18n/en/common.json";
import enFocus from "@/i18n/en/focus.json";
import enProcess from "@/i18n/en/process.json";
import enSettings from "@/i18n/en/settings.json";
import enToast from "@/i18n/en/toast.json";
import enTray from "@/i18n/en/tray.json";
import jaCommon from "@/i18n/ja/common.json";
import jaFocus from "@/i18n/ja/focus.json";
import jaProcess from "@/i18n/ja/process.json";
import jaSettings from "@/i18n/ja/settings.json";
import jaToast from "@/i18n/ja/toast.json";
import jaTray from "@/i18n/ja/tray.json";
import koCommon from "@/i18n/ko/common.json";
import koFocus from "@/i18n/ko/focus.json";
import koProcess from "@/i18n/ko/process.json";
import koSettings from "@/i18n/ko/settings.json";
import koToast from "@/i18n/ko/toast.json";
import koTray from "@/i18n/ko/tray.json";
import zhCommon from "@/i18n/zh/common.json";
import zhFocus from "@/i18n/zh/focus.json";
import zhProcess from "@/i18n/zh/process.json";
import zhSettings from "@/i18n/zh/settings.json";
import zhToast from "@/i18n/zh/toast.json";
import zhTray from "@/i18n/zh/tray.json";

export type SupportedLanguage = "en" | "ko" | "ja" | "zh";

export type TranslationValues = Record<string, string | number>;

type TranslationMap = Record<SupportedLanguage, Record<string, string>>;

export const SUPPORTED_LANGUAGES: SupportedLanguage[] = ["en", "ko", "ja", "zh"];

export const LANGUAGE_STORAGE_KEY = "langcon.language";

export const FALLBACK_LANGUAGE: SupportedLanguage = "en";

const translations: TranslationMap = {
  en: { ...enCommon, ...enFocus, ...enProcess, ...enSettings, ...enToast, ...enTray },
  ko: { ...koCommon, ...koFocus, ...koProcess, ...koSettings, ...koToast, ...koTray },
  ja: { ...jaCommon, ...jaFocus, ...jaProcess, ...jaSettings, ...jaToast, ...jaTray },
  zh: { ...zhCommon, ...zhFocus, ...zhProcess, ...zhSettings, ...zhToast, ...zhTray },
};

const interpolate = (template: string, values?: TranslationValues) => {
  if (!values) return template;
  return template.replace(/\{\{(\w+)\}\}/g, (_, key) => String(values[key] ?? `{{${key}}}`));
};

const matchLanguage = (value: string | null | undefined): SupportedLanguage | null => {
  if (!value) return null;
  const lower = value.toLowerCase();
  const exact = SUPPORTED_LANGUAGES.find((lang) => lower === lang);
  if (exact) return exact;
  const prefix = SUPPORTED_LANGUAGES.find((lang) => lower.startsWith(`${lang}-`));
  return prefix ?? null;
};

const loadStoredLanguage = (): SupportedLanguage | null => {
  if (typeof window === "undefined") return null;
  try {
    return matchLanguage(localStorage.getItem(LANGUAGE_STORAGE_KEY));
  } catch {
    return null;
  }
};

const detectNavigatorLanguage = (): SupportedLanguage | null => {
  if (typeof navigator === "undefined") return null;
  const preferred = Array.isArray(navigator.languages) && navigator.languages.length > 0
    ? navigator.languages
    : [navigator.language];
  for (const locale of preferred) {
    const match = matchLanguage(locale);
    if (match) return match;
  }
  return null;
};

export const detectInitialLanguage = (): SupportedLanguage => {
  return loadStoredLanguage() ?? detectNavigatorLanguage() ?? FALLBACK_LANGUAGE;
};

export const storeLanguage = (language: SupportedLanguage) => {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
  } catch {
    // ignore storage failures
  }
};

export const translate = (language: SupportedLanguage, key: string, values?: TranslationValues) => {
  const template = translations[language]?.[key] ?? translations[FALLBACK_LANGUAGE]?.[key];
  if (!template) return key;
  return interpolate(template, values);
};

export const getLanguageLabelKey = (language: SupportedLanguage) => `settings.language.options.${language}`;
