import { createApp } from 'vue'
import DynamicIsland from "./components/DynamicIsland.vue"
import App from './App.vue'
import TrayMenu from './components/TrayMenu.vue'
import UpdateNotice from './components/UpdateNotice.vue'
import './style.css'

const view = new URLSearchParams(window.location.search).get('view')
if (view === 'tray' || view === 'update-notice') document.documentElement.classList.add('tray-page')
if (view === "island") document.documentElement.classList.add("island-page")
createApp(view === "island" ? DynamicIsland : view === 'tray' ? TrayMenu : view === 'update-notice' ? UpdateNotice : App).mount('#app')
