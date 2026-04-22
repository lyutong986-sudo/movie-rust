import {
  useNavigatorLanguage,
  watchImmediate
} from '@vueuse/core';
import { computed } from 'vue';
import { isNil, sealed } from '@jellyfin-vue/shared/validation';
import type { KeysOfUnion } from 'type-fest';
import i18next from 'i18next';
import { languages } from '@jellyfin-vue/i18n';
import { vuetify } from '#/plugins/vuetify.ts';
import { SyncedStore } from '#/store/super/synced-store.ts';

/**
 * == INTERFACES AND TYPES ==
 * Casted typings for the CustomPrefs property of DisplayPreferencesDto
 */

export interface ClientSettingsState {
  locale?: string;
}

@sealed
class ClientSettingsStore extends SyncedStore<ClientSettingsState, KeysOfUnion<ClientSettingsState>> {
  private readonly _navigatorLanguage = useNavigatorLanguage();
  private readonly _resolveSupportedLocale = (locale?: string): string | undefined => {
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
  };
  private readonly _BROWSER_LANGUAGE = computed(() =>
    this._resolveSupportedLocale(this._navigatorLanguage.language.value)
  );

  /**
   * @param mode - If setting to undefined, auto locale is used
   */
  public readonly locale = computed({
    get: () => this._state.value.locale,
    set: (newVal?: string) => {
      const isAuto = isNil(newVal) || !languages.includes(newVal);

      this._state.value.locale = isAuto ? undefined : newVal;
    }
  });

  /**
   * == METHODS ==
   */
  private readonly _updateLocale = (): void => {
    const targetLocale = isNil(this.locale.value)
      ? this._BROWSER_LANGUAGE.value
      : this._resolveSupportedLocale(this.locale.value);

    if (targetLocale) {
      vuetify.locale.current.value = targetLocale;
      void i18next.changeLanguage(targetLocale);
    }
  };

  public constructor() {
    super({
      defaultState: () => ({
        locale: undefined
      }),
      storeKey: 'clientSettings',
      resetOnLogout: true,
      persistenceType: 'localStorage'
    });
    /**
     * == WATCHERS ==
     */

    /**
     * Locale change
     */
    watchImmediate(
      [this._BROWSER_LANGUAGE, this.locale],
      this._updateLocale
    );
  }
}

export const clientSettings = new ClientSettingsStore();
