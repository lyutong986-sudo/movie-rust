import type { NavigationGuardReturn, RouteLocationNormalized } from 'vue-router';
import { isStr } from '@jellyfin-vue/shared/validation';
import i18next from 'i18next';
import { useSnackbar } from '#/composables/use-snackbar.ts';

/**
 * Validates that the route has a correct itemId parameter by checking that the parameter is a valid
 * Emby/Jellyfin compatible item identifier.
 */
export function validateGuard(
  to: RouteLocationNormalized
): NavigationGuardReturn {
  if (('itemId' in to.params) && isStr(to.params.itemId)) {
    const check
      = /^[\da-f]{32}$/i.test(to.params.itemId)
        || /^[\da-f]{8}-[\da-f]{4}-[\da-f]{4}-[\da-f]{4}-[\da-f]{12}$/i.test(to.params.itemId);

    if (!check) {
      useSnackbar(i18next.t('routeValidationError'), 'error');

      return false;
    }
  }
}
