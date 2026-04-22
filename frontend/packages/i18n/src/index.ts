import i18next, { type BackendModule } from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { NOOP } from '@vue/shared';
import { resources } from 'virtual:i18next/resources';

/**
 * In @jellyfin-vue/frontend, see #/store/clientSettings to check where the current user language is initialised
 */

const DEFAULT_LANGUAGE = 'en';

export const languages = Object.freeze(Object.keys(resources));

await i18next
  .use(LanguageDetector)
  /**
   * Lazy load the used locales only to save memory
   */
  .use({
    type: 'backend',
    init: NOOP,
    read: (language, _, done) => {
      const loadFn = resources[language];

      if (loadFn) {
        void (async () => done(undefined, await loadFn()))();
      } else {
        done(new Error(`Language ${language} not found`), {});
      }
    }
  } satisfies BackendModule)
  .init({
    fallbackLng: DEFAULT_LANGUAGE,
    debug: import.meta.env.DEV,
    nonExplicitSupportedLngs: true,
    supportedLngs: languages,
    detection: {
      order: [
        'querystring',
        'hash',
        'localStorage',
        'sessionStorage',
        'cookie',
        'navigator',
        'htmlTag'
      ],
      caches: ['localStorage', 'cookie'],
      lookupQuerystring: 'lng',
      lookupHash: 'lng',
      lookupLocalStorage: 'i18nextLng',
      lookupSessionStorage: 'i18nextLng',
      lookupCookie: 'i18next',
      htmlTag: globalThis.document?.documentElement,
      cookieOptions: {
        path: '/',
        sameSite: 'strict'
      }
    },
    interpolation: {
      escapeValue: false
    }
  });

export function resolveSupportedLanguage(locale?: string): string | undefined {
  if (!locale) {
    return undefined;
  }

  const normalized = locale.trim().toLowerCase();
  const exact = languages.find(language => language.toLowerCase() === normalized);

  if (exact) {
    return exact;
  }

  const baseLanguage = normalized.split('-')[0];
  const baseMatch = languages.find(language => language.toLowerCase() === baseLanguage);

  if (baseMatch) {
    return baseMatch;
  }

  return languages.find(language => language.toLowerCase().startsWith(`${baseLanguage}-`));
}
