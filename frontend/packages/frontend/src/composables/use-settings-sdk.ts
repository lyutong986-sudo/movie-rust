import type { AxiosRequestConfig } from 'axios';
import type {
  ActivityLogEntry,
  AuthenticationInfo,
  BrandingBrandingOptions,
  DevicesDeviceInfo,
  DevicesDeviceOptions,
  GlobalizationLocalizatonOption,
  LogFile,
  QueryResultActivityLogEntry,
  QueryResultDevicesDeviceInfo,
  QueryResultString,
  ServerConfiguration,
  SystemInfo
} from '@jellyfin/sdk/lib/generated-client';
import type { ImageApiPostUserImageRequest } from '@jellyfin/sdk/lib/generated-client/api/image-api';
import type { UserApiUpdateUserPasswordRequest } from '@jellyfin/sdk/lib/generated-client/api/user-api';
import { getApiKeyApi } from '@jellyfin/sdk/lib/utils/api/api-key-api';
import { getBrandingApi } from '@jellyfin/sdk/lib/utils/api/branding-api';
import { getConfigurationApi } from '@jellyfin/sdk/lib/utils/api/configuration-api';
import { getDevicesApi } from '@jellyfin/sdk/lib/utils/api/devices-api';
import { getImageApi } from '@jellyfin/sdk/lib/utils/api/image-api';
import { getLocalizationApi } from '@jellyfin/sdk/lib/utils/api/localization-api';
import { getSystemApi } from '@jellyfin/sdk/lib/utils/api/system-api';
import { getUserApi } from '@jellyfin/sdk/lib/utils/api/user-api';
import auth from '#/plugins/remote/auth.ts';
import { remote } from '#/plugins/remote/index.ts';
import { getSdkSystemLogUrl } from '#/utils/sdk-url.ts';

export type SettingsDeviceDetails = DevicesDeviceInfo & {
  ReportedDeviceId?: string;
};

export type SettingsSessionInfo = {
  Id: string;
  UserName?: string;
  Client?: string;
  DeviceId?: string;
  DeviceName?: string;
  ApplicationVersion?: string;
  RemoteEndPoint?: string;
  SupportsRemoteControl?: boolean;
  SupportedCommands?: string[];
  NowPlayingItem?: {
    Name?: string;
  };
  PlayState?: {
    IsPaused?: boolean;
  };
};

export type SettingsMediaPathInfo = {
  Path: string;
  NetworkPath?: string | null;
  Username?: string;
  Password?: string;
};

export type SettingsLibraryOptions = {
  Enabled?: boolean;
  EnableArchiveMediaFiles?: boolean;
  EnablePhotos?: boolean;
  EnableRealtimeMonitor?: boolean;
  EnableChapterImageExtraction?: boolean;
  ExtractChapterImagesDuringLibraryScan?: boolean;
  SaveLocalMetadata?: boolean;
  EnableInternetProviders?: boolean;
  DownloadImagesInAdvance?: boolean;
  ImportMissingEpisodes?: boolean;
  EnableAutomaticSeriesGrouping?: boolean;
  EnableEmbeddedTitles?: boolean;
  EnableEmbeddedEpisodeInfos?: boolean;
  AutomaticRefreshIntervalDays?: number;
  PreferredMetadataLanguage?: string | null;
  MetadataCountryCode?: string | null;
  SeasonZeroDisplayName?: string;
  MetadataSavers?: string[];
  DisabledLocalMetadataReaders?: string[];
  LocalMetadataReaderOrder?: string[];
  PathInfos?: SettingsMediaPathInfo[];
} & Record<string, unknown>;

export type SettingsLibraryOptionInfo = {
  Name?: string;
  SetupUrl?: string;
  DefaultEnabled?: boolean;
  Features?: string[];
};

export type SettingsLibraryOptionsResult = {
  MetadataSavers?: SettingsLibraryOptionInfo[];
  MetadataReaders?: SettingsLibraryOptionInfo[];
  SubtitleFetchers?: SettingsLibraryOptionInfo[];
  LyricsFetchers?: SettingsLibraryOptionInfo[];
  TypeOptions?: unknown[];
  DefaultLibraryOptions?: SettingsLibraryOptions;
};

export type SettingsVirtualFolderInfo = {
  Name: string;
  CollectionType: string;
  ItemId: string;
  Locations: string[];
  LibraryOptions?: SettingsLibraryOptions;
};

export type SettingsSelectableMediaFolder = {
  Name: string;
  Id: string;
  Guid?: string;
  SubFolders?: Array<{
    Name: string;
    Id: string;
    Path?: string;
    IsUserAccessConfigurable?: boolean;
  }>;
  IsUserAccessConfigurable?: boolean;
};

export type SettingsTaskInfo = {
  Id?: string;
  Name?: string;
  Description?: string;
  Category?: string;
  State?: string;
  CurrentProgressPercentage?: number;
  LastExecutionResult?: {
    StartTimeUtc?: string;
    EndTimeUtc?: string;
    Status?: string;
    Name?: string;
    Key?: string;
    Id?: string;
    ErrorMessage?: string | null;
    LongErrorMessage?: string | null;
  };
  Triggers?: unknown[];
  IsHidden?: boolean;
  IsEnabled?: boolean;
  Key?: string;
};

export type SettingsNetEndPointInfo = {
  IsLocal?: boolean;
  IsInNetwork?: boolean;
};

export type SettingsWakeOnLanInfo = {
  MacAddress?: string;
  BroadcastAddress?: string;
  Port?: number;
};

export type SettingsServerDomain = {
  name?: string;
  url: string;
  isLocal?: boolean;
  isRemote?: boolean;
};

export type SettingsPluginInfo = {
  Id: string;
  Name: string;
  Version?: string;
  Description?: string;
  Enabled?: boolean;
};

export type SettingsActiveEncoding = {
  Id: string;
  PlaySessionId?: string;
  ItemId?: string;
  State?: string;
  Progress?: number;
};

export function useSettingsSdk() {
  const sdkAxios = () => {
    const axios = remote.sdk.api!.axiosInstance;
    const token = auth.currentUserToken.value;

    if (token) {
      axios.defaults.headers.common['X-Emby-Token'] = token;
    }

    return axios;
  };

  const accountApi = {
    deleteUserImage: (userId: string) =>
      (remote.sdk.newUserApi(getImageApi) as {
        deleteUserImage: (payload: { userId: string }) => Promise<unknown>;
      }).deleteUserImage({ userId }),
    postUserImage: (payload: ImageApiPostUserImageRequest, config?: AxiosRequestConfig) =>
      (remote.sdk.newUserApi(getImageApi) as {
        postUserImage: (request: ImageApiPostUserImageRequest, config?: AxiosRequestConfig) => Promise<unknown>;
      }).postUserImage(payload, config),
    updateUserPassword: (payload: UserApiUpdateUserPasswordRequest & { userId: string }) =>
      (remote.sdk.newUserApi(getUserApi) as {
        updateUserPassword: (request: UserApiUpdateUserPasswordRequest & { userId: string }) => Promise<unknown>;
      }).updateUserPassword(payload)
  };

  const logsApi = {
    getLogs: async (): Promise<LogFile[]> =>
      ((await (remote.sdk.newUserApi(getSystemApi) as {
        getServerLogs: () => Promise<{ data: LogFile[] }>;
      }).getServerLogs()).data ?? []),
    getLogLines: async (name: string): Promise<QueryResultString> =>
      ((await sdkAxios().get<QueryResultString>(
        `/System/Logs/${encodeURIComponent(name)}/Lines`,
        { params: { Limit: 200 } }
      )).data),
    getLogFileUrl: (name: string): string | undefined =>
      getSdkSystemLogUrl(name),
    getActivityLogEntries: async (limit = 50): Promise<QueryResultActivityLogEntry> =>
      ((await sdkAxios().get<QueryResultActivityLogEntry>('/System/ActivityLog/Entries', {
        params: { Limit: limit }
      })).data ?? { Items: [] as ActivityLogEntry[] })
  };

  const devicesApi = remote.sdk.newUserApi(getDevicesApi) as {
    getDevices: () => Promise<{ data: QueryResultDevicesDeviceInfo }>;
    getDevicesInfo: (id: string) => Promise<{ data: SettingsDeviceDetails }>;
    getDevicesOptions: (id: string) => Promise<{ data: DevicesDeviceOptions }>;
    postDevicesOptions: (body: DevicesDeviceOptions, id: string) => Promise<unknown>;
    deleteDevice: (payload: { id: string }) => Promise<unknown>;
  };

  const apiKeysApi = {
    getKeys: async (): Promise<AuthenticationInfo[]> =>
      ((await (remote.sdk.newUserApi(getApiKeyApi) as {
        getKeys: () => Promise<{ data: { Items?: AuthenticationInfo[] } }>;
      }).getKeys()).data.Items ?? []),
    createKey: (app: string) =>
      (remote.sdk.newUserApi(getApiKeyApi) as {
        createKey: (payload: { app: string }) => Promise<unknown>;
      }).createKey({ app }),
    revokeKey: (key: string) =>
      (remote.sdk.newUserApi(getApiKeyApi) as {
        revokeKey: (payload: { key: string }) => Promise<unknown>;
      }).revokeKey({ key })
  };

  const sessionsApi = {
    getSessions: async (): Promise<SettingsSessionInfo[]> =>
      ((await sdkAxios().get<SettingsSessionInfo[]>('/Sessions')).data ?? []),
    sendPlayingCommand: (id: string, command: string) =>
      sdkAxios().post(`/Sessions/${id}/Playing/${command}`, {
        Name: command,
        Command: command
      }),
    sendMessage: (id: string, payload: { Header?: string; Text?: string; TimeoutMs?: number }) =>
      sdkAxios().post(`/Sessions/${id}/Message`, payload)
  };

  const librariesApi = {
    getLibraryVirtualfoldersQuery: async (): Promise<SettingsVirtualFolderInfo[]> => {
      const { data } = await sdkAxios().get<
        SettingsVirtualFolderInfo[] | { Items?: SettingsVirtualFolderInfo[] }
      >('/Library/VirtualFolders/Query');

      return Array.isArray(data) ? data : data.Items ?? [];
    },
    getLibrarySelectablemediafolders: async (): Promise<SettingsSelectableMediaFolder[]> => {
      const { data } = await sdkAxios().get<
        SettingsSelectableMediaFolder[] | { Items?: SettingsSelectableMediaFolder[] }
      >('/Library/SelectableMediaFolders');

      return Array.isArray(data) ? data : data.Items ?? [];
    },
    getLibrariesAvailableoptions: async (): Promise<SettingsLibraryOptionsResult> =>
      ((await sdkAxios().get<SettingsLibraryOptionsResult>('/Libraries/AvailableOptions')).data ?? {}),
    postLibraryRefresh: () =>
      sdkAxios().post('/Library/Refresh'),
    postLibraryVirtualfolders: (body: {
      Name?: string;
      CollectionType?: string;
      RefreshLibrary?: boolean;
      Paths?: string[];
      LibraryOptions?: SettingsLibraryOptions;
    }) =>
      sdkAxios().post('/Library/VirtualFolders', body, {
        params: {
          Name: body.Name,
          CollectionType: body.CollectionType,
          RefreshLibrary: body.RefreshLibrary,
          Paths: body.Paths?.join(',')
        }
      }),
    postLibraryVirtualfoldersName: (body: { Id?: string; Name?: string; NewName?: string }) =>
      sdkAxios().post('/Library/VirtualFolders/Name', body, {
        params: body
      }),
    postLibraryVirtualfoldersLibraryoptions: (body: { Id: string; LibraryOptions: SettingsLibraryOptions }) =>
      sdkAxios().post('/Library/VirtualFolders/LibraryOptions', body),
    postLibraryVirtualfoldersPaths: (body: {
      Name?: string;
      Id?: string;
      Path?: string;
      PathInfo?: SettingsMediaPathInfo;
      RefreshLibrary?: boolean;
    }) =>
      sdkAxios().post('/Library/VirtualFolders/Paths', body),
    deleteLibraryVirtualfoldersPaths: (body: { Name?: string; Id?: string; Path?: string; RefreshLibrary?: boolean }) =>
      sdkAxios().delete('/Library/VirtualFolders/Paths', {
        params: body,
        data: body
      }),
    deleteLibraryVirtualfolders: (body: { Name?: string; Id?: string; RefreshLibrary?: boolean }) =>
      sdkAxios().delete('/Library/VirtualFolders', {
        params: body,
        data: body
      }),
    postLibraryVirtualfoldersDelete: (body: { Name?: string; Id?: string; RefreshLibrary?: boolean }) =>
      sdkAxios().post('/Library/VirtualFolders/Delete', body, {
        params: body
      })
  };

  const scheduledTasksApi = {
    getScheduledtasks: async (isHidden?: boolean, isEnabled?: boolean): Promise<SettingsTaskInfo[]> =>
      ((await sdkAxios().get<SettingsTaskInfo[]>('/ScheduledTasks', {
        params: {
          IsHidden: isHidden,
          IsEnabled: isEnabled
        }
      })).data ?? []),
    getScheduledtasksById: async (id: string): Promise<SettingsTaskInfo> =>
      ((await sdkAxios().get<SettingsTaskInfo>(`/ScheduledTasks/${id}`)).data),
    postScheduledtasksRunningById: (id: string) =>
      sdkAxios().post(`/ScheduledTasks/Running/${id}`),
    postScheduledtasksRunningByIdDelete: (id: string) =>
      sdkAxios().post(`/ScheduledTasks/Running/${id}/Delete`)
  };

  const pluginsApi = {
    getPlugins: async (): Promise<SettingsPluginInfo[]> =>
      ((await sdkAxios().get<SettingsPluginInfo[]>('/Plugins')).data ?? []),
    getPluginsByIdConfiguration: async (id: string): Promise<unknown> =>
      ((await sdkAxios().get<unknown>(`/Plugins/${id}/Configuration`)).data),
    postPluginsByIdConfiguration: (id: string, body: unknown) =>
      sdkAxios().post(`/Plugins/${id}/Configuration`, body),
    postPluginsByIdDelete: (id: string) =>
      sdkAxios().post(`/Plugins/${id}/Delete`),
    deletePluginsById: (id: string) =>
      sdkAxios().delete(`/Plugins/${id}`)
  };

  const transcodingApi = {
    getVideosActiveEncodings: async (): Promise<SettingsActiveEncoding[]> =>
      ((await sdkAxios().get<SettingsActiveEncoding[]>('/Videos/ActiveEncodings')).data ?? []),
    deleteVideosActiveEncodings: (id: string) =>
      sdkAxios().delete('/Videos/ActiveEncodings', {
        params: { Id: id }
      }),
    postVideosActiveEncodingsDelete: (id: string) =>
      sdkAxios().post('/Videos/ActiveEncodings/Delete', undefined, {
        params: { Id: id }
      })
  };

  const serverApi = {
    getLocalizationOptions: async (): Promise<GlobalizationLocalizatonOption[]> =>
      ((await (remote.sdk.newUserApi(getLocalizationApi) as {
        getLocalizationOptions: () => Promise<{ data: GlobalizationLocalizatonOption[] }>;
      }).getLocalizationOptions()).data ?? []),
    getConfiguration: async (): Promise<ServerConfiguration> =>
      ((await (remote.sdk.newUserApi(getConfigurationApi) as {
        getConfiguration: () => Promise<{ data: ServerConfiguration }>;
      }).getConfiguration()).data),
    updateConfiguration: (body: ServerConfiguration) =>
      (remote.sdk.newUserApi(getConfigurationApi) as {
        updateConfiguration: (payload: { serverConfiguration: ServerConfiguration }) => Promise<unknown>;
      }).updateConfiguration({ serverConfiguration: body }),
    updateNamedConfiguration: (key: string, body: unknown) =>
      (remote.sdk.newUserApi(getConfigurationApi) as {
        updateNamedConfiguration: (payload: { key: string; body: string }) => Promise<unknown>;
      }).updateNamedConfiguration({
        key,
        body: typeof body === 'string' ? body : JSON.stringify(body)
      }),
    getBrandingOptions: async (): Promise<BrandingBrandingOptions> =>
      ((await (remote.sdk.newUserApi(getBrandingApi) as {
        getBrandingOptions: () => Promise<{ data: BrandingBrandingOptions }>;
      }).getBrandingOptions()).data),
    getSystemInfo: async (): Promise<SystemInfo> =>
      ((await sdkAxios().get<SystemInfo>('/System/Info')).data),
    getSystemEndpoint: async (): Promise<SettingsNetEndPointInfo> =>
      ((await sdkAxios().get<SettingsNetEndPointInfo>('/System/Endpoint')).data),
    getSystemWakeonlaninfo: async (): Promise<SettingsWakeOnLanInfo[]> =>
      ((await sdkAxios().get<SettingsWakeOnLanInfo[]>('/System/WakeOnLanInfo')).data ?? []),
    getServerDomains: async (): Promise<SettingsServerDomain[]> => {
      const { data } = await sdkAxios().get<SettingsServerDomain[] | { data?: SettingsServerDomain[] }>(
        '/System/Ext/ServerDomains'
      );

      return Array.isArray(data) ? data : data.data ?? [];
    }
  };

  return {
    accountApi,
    logsApi,
    devicesApi,
    apiKeysApi,
    sessionsApi,
    librariesApi,
    scheduledTasksApi,
    pluginsApi,
    transcodingApi,
    serverApi
  };
}
