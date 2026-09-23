/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import path from 'path'
import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  // Same switch as src-tauri/src/dev.rs: the CMS owns the window title, so a dev build marks it here.
  define: {
    __BLOCKFIELD_DEV_BUILD__: JSON.stringify(
      ['1', 'true', 'yes', 'on'].includes(process.env.BLOCKFIELD_DEV_BUILD?.trim() ?? ''),
    ),
  },
  plugins: [
    // The React and Tailwind plugins are both required for Make, even if
    // Tailwind is not being actively used – do not remove them
    react(),
    tailwindcss(),
  ],
  resolve: {
    alias: {
      // Alias @ to the src directory
      '@': path.resolve(__dirname, './src'),
    },
  },

  // File types to support raw imports. Never add .css, .tsx, or .ts files to this.
  assetsInclude: ['**/*.svg', '**/*.csv'],

  test: {
    globals: true,
    css: true,
  },
})
