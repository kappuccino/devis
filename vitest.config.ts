import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.{ts,js}"],
    // Les tests d'intégration de la documentation génèrent et indexent des PDF.
    testTimeout: 120_000,
  },
});
