import type { KeysOfUnion } from 'type-fest';
import { sealed } from '@jellyfin-vue/shared/validation';
import { SyncedStore } from '#/store/super/synced-store.ts';

export interface HomeSettingsState {
  showResume: boolean;
  showNextUp: boolean;
  showLatest: boolean;
  latestLimit: number;
  sections: string[];
}

@sealed
class HomeSettingsStore extends SyncedStore<HomeSettingsState, KeysOfUnion<HomeSettingsState>> {
  public constructor() {
    super({
      storeKey: 'homeSettings',
      defaultState: () => ({
        showResume: true,
        showNextUp: true,
        showLatest: true,
        latestLimit: 16,
        sections: ['libraries', 'resumevideo', 'nextup', 'latestmedia']
      }),
      persistenceType: 'localStorage',
      resetOnLogout: true
    });
  }
}

export const homeSettings = new HomeSettingsStore();
