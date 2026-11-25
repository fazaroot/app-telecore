/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        bg: '#F4F6FB',
        primary: '#526FE5',
        secondary: '#7187ED',
        surface: '#FFFFFF',
        ink: '#20283A',
        success: '#25A56A',
        danger: '#E05260',
        darkbg: '#101522',
        darksurface: '#1A2233',
        darktext: '#E8EDF7',
      },
    },
  },
  plugins: [],
}
