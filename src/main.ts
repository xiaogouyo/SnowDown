import { createApp } from 'vue'
import App from './App.vue'
import TrayMenu from './components/TrayMenu.vue'
import './style.css'

const tray = new URLSearchParams(window.location.search).get('view') === 'tray'
if (tray) document.documentElement.classList.add('tray-page')
createApp(tray ? TrayMenu : App).mount('#app')
