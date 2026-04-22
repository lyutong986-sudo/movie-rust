/**
 * Instantiates the Axios instance used for the SDK and requests
 */
import axios, {
  type AxiosError
} from 'axios';
import { sealed } from '@jellyfin-vue/shared/validation';
import i18next from 'i18next';
import auth from './auth.ts';
import { useSnackbar } from '#/composables/use-snackbar.ts';

@sealed
class RemotePluginAxios {
  public readonly instance = axios.create();
  private readonly _defaults = this.instance.defaults;
  private _logoutPromise: Promise<void> | undefined;

  public resetDefaults(): void {
    this.instance.defaults = this._defaults;
  }

  /**
   * Intercepts 401 (Unathorized) error code and logs out the user inmmediately,
   * as the session probably has been revoked remotely.
   */
  public logoutInterceptor = async (error: AxiosError): Promise<void> => {
    const isUnauthorized = error.response?.status === 401;
    const hasCurrentUser = Boolean(auth.currentUser.value);
    const requestUrl = error.config?.url ?? '';

    if (
      isUnauthorized
      && hasCurrentUser
      && !requestUrl.includes('/Sessions/Logout')
    ) {
      if (!this._logoutPromise) {
        this._logoutPromise = (async () => {
          try {
            if (requestUrl.includes('/Users/Me')) {
              throw error;
            }

            await auth.refreshCurrentUserInfo();
          } catch {
            await auth.logoutCurrentUser(true);
            useSnackbar(i18next.t('kickedOut'), 'error');
          } finally {
            this._logoutPromise = undefined;
          }
        })();
      }

      await this._logoutPromise;
    }

    /**
     * Pass the error so it's handled in try/catch blocks afterwards
     */
    throw error;
  };

  public constructor() {
    this.instance.interceptors.response.use(
      undefined,
      this.logoutInterceptor
    );
  }
}

const RemotePluginAxiosInstance = new RemotePluginAxios();

export default RemotePluginAxiosInstance;
