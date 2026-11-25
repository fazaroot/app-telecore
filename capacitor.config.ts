import type { CapacitorConfig } from '@capacitor/cli';

const config: CapacitorConfig = {
  appId: 'com.telecore.app',
  appName: 'TeleCore',
  webDir: 'frontend/dist',
  server: {
    androidScheme: 'https',
  },
  plugins: {
    SplashScreen: {
      launchShowDuration: 1500,
      backgroundColor: '#526FE5',
    },
    StatusBar: {
      style: 'light',
      backgroundColor: '#526FE5',
      // WebView TIDAK menutupi status bar — header tidak tertutup ikon
      // status bar (bug POCO M6 v1.0) dan safe-area-inset-top bernilai 0.
      overlaysWebView: false,
    },
    // LocalNotifications: smallIcon sengaja tidak diisi — tidak ada resource
    // ic_stat_icon_config_sample di proyek ini; plugin memakai ikon default.
  },
};

export default config;
