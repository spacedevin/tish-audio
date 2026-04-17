import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Discover inputs dynamically based on emitted .app.html files
const inputs: Record<string, string> = {};
const files = fs.readdirSync(__dirname);
for (const file of files) {
  if (file.endsWith('.app.html')) {
    const id = path.basename(file, '.app.html');
    inputs[id] = path.resolve(__dirname, file);
  }
}

// Fallback if no files generated yet (so vite doesn't fail on initial load)
if (Object.keys(inputs).length === 0) {
  inputs['main'] = 'index.html'; 
}

export default defineConfig({
  plugins: [
    react(), 
    tailwindcss(),
    {
      name: 'dev-html-rewrite',
      configureServer(server) {
        server.middlewares.use((req, res, next) => {
          // In dev mode, if the user visits /15.html, serve the generated /15.app.html
          // instead of serving the raw JSX source code that happens to be named .html
          if (req.url && req.url.endsWith('.html') && !req.url.endsWith('.app.html')) {
            const urlObj = new URL(req.url, `http://${req.headers.host || 'localhost'}`);
            const appHtmlPath = urlObj.pathname.replace(/\.html$/, '.app.html');
            
            // Check if the mapped .app.html exists in the project root
            if (fs.existsSync(path.join(__dirname, appHtmlPath))) {
              // Rewrite the request url to serve the generated entry
              req.url = appHtmlPath + urlObj.search;
            }
          }
          next();
        });
      }
    }
  ],
  build: {
    rollupOptions: {
      input: inputs
    }
  }
});
