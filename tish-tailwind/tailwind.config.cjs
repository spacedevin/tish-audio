/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./index.html",
    "./src/**/*.tish",
    "./src/modules/**/*.{tish,tsx,js}",
    "./tish-tailwind/src/**/*.tish",
    "./16.html",
  ],
  theme: {
    extend: {},
  },
  plugins: [],
};
