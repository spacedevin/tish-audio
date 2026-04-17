import fs from 'fs/promises';
import path from 'path';

const SRC_ENTRIES = 'src/entries';
const SRC_MAINS = 'src/mains';

async function main() {
  await fs.mkdir(SRC_ENTRIES, { recursive: true });
  await fs.mkdir(SRC_MAINS, { recursive: true });

  const files = await fs.readdir('.');
  const htmlFiles = files.filter(f => f.endsWith('.html') && !f.endsWith('.app.html'));

  for (const file of htmlFiles) {
    const content = await fs.readFile(file, 'utf-8');
    const lines = content.split('\n');
    const firstNonEmptyLine = lines.find(line => line.trim().length > 0);
    
    // Check if it's a React file
    if (firstNonEmptyLine && firstNonEmptyLine.includes('import React')) {
      const id = path.basename(file, '.html');
      
      // 1. Copy to src/entries/<id>.jsx
      const entryPath = path.join(SRC_ENTRIES, `${id}.jsx`);
      await fs.writeFile(entryPath, content);
      
      // 2. Emit src/mains/<id>.jsx
      const mainPath = path.join(SRC_MAINS, `${id}.jsx`);
      const mainContent = `import React, { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '../index.css';
import App from '../entries/${id}.jsx';

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <App />
  </StrictMode>
);
`;
      await fs.writeFile(mainPath, mainContent);
      
      // 3. Emit root <id>.app.html
      const appHtmlPath = `${id}.app.html`;
      const appHtmlContent = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>App ${id}</title>
</head>
<body>
  <div id="root"></div>
  <script type="module" src="/src/mains/${id}.jsx"></script>
</body>
</html>`;
      await fs.writeFile(appHtmlPath, appHtmlContent);
      console.log(`Synced React entry for ${id}`);
    }
  }
}

main().catch(console.error);