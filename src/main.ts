import { createApp } from 'vue'
import App from './App.vue'
import TrayMenu from './components/TrayMenu.vue'
import UpdateNotice from './components/UpdateNotice.vue'
import './style.css'

const view = new URLSearchParams(window.location.search).get('view')
if (view === 'tray' || view === 'update-notice') document.documentElement.classList.add('tray-page')
createApp(view === 'tray' ? TrayMenu : view === 'update-notice' ? UpdateNotice : App).mount('#app')
