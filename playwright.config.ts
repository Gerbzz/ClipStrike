import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/playwright',
  timeout: 30_000,
  retries: 0,
  use: {
    baseURL: 'http://127.0.0.1:8080',
    headless: false,
    launchOptions: {
      args: [
        '--enable-unsafe-webgpu',
        '--ignore-gpu-blocklist',
        '--enable-features=Vulkan,UseSkiaRenderer,WebGPU,AllowUnsafeWebGPU',
        '--use-angle=swiftshader',
      ],
    },
  },
  reporter: [['list']],
});

