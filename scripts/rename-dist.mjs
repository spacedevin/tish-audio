import fs from 'fs/promises';
import path from 'path';

async function main() {
  const distDir = 'dist';
  try {
    const files = await fs.readdir(distDir);
    for (const file of files) {
      if (file.endsWith('.app.html')) {
        const id = path.basename(file, '.app.html');
        await fs.rename(path.join(distDir, file), path.join(distDir, `${id}.html`));
        console.log(`Renamed ${file} to ${id}.html in dist`);
      }
    }
  } catch (e) {
    if (e.code !== 'ENOENT') throw e;
  }
}

main().catch(console.error);
