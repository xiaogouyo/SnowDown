<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'
import { Check, ChevronDown } from '@lucide/vue'

const props = withDefaults(defineProps<{ modelValue: string | number; options: { value: string | number; label: string }[]; label: string; disabled?: boolean; compact?: boolean }>(), { disabled: false, compact: false })
const emit = defineEmits<{ 'update:modelValue': [value: string | number] }>()
const expanded = ref(false)
const trigger = ref<HTMLButtonElement>()
const popup = ref<HTMLElement>()
const active = ref(0)
const position = ref({ left: '0px', top: '0px', width: '160px', maxHeight: '260px' })
const selected = computed(() => props.options.find(option => option.value === props.modelValue)?.label ?? '')
const id = `select-${Math.random().toString(36).slice(2)}`

function place() {
  const rect = trigger.value?.getBoundingClientRect()
  if (!rect) return
  const width = Math.min(Math.max(rect.width, props.compact ? 150 : 180), window.innerWidth - 24)
  const height = Math.min(props.options.length * 39 + 12, 260)
  const below = window.innerHeight - rect.bottom - 16
  const top = below >= height ? rect.bottom + 7 : Math.max(12, rect.top - height - 7)
  position.value = { left: `${Math.min(rect.left, window.innerWidth - width - 12)}px`, top: `${top}px`, width: `${width}px`, maxHeight: `${Math.max(90, Math.min(height, window.innerHeight - top - 12))}px` }
}
async function open() {
  if (props.disabled || !props.options.length) return
  expanded.value = true; active.value = Math.max(0, props.options.findIndex(option => option.value === props.modelValue)); place()
  document.addEventListener('pointerdown', outside)
  window.addEventListener('resize', close)
  window.addEventListener('scroll', onScroll, true)
  await nextTick(); popup.value?.focus()
}
function close() {
  expanded.value = false
  document.removeEventListener('pointerdown', outside)
  window.removeEventListener('resize', close)
  window.removeEventListener('scroll', onScroll, true)
}
function outside(event: PointerEvent) { if (!popup.value?.contains(event.target as Node) && !trigger.value?.contains(event.target as Node)) close() }
function onScroll(event: Event) { if (!popup.value?.contains(event.target as Node)) close() }
function choose(index: number) { const item = props.options[index]; if (item) emit('update:modelValue', item.value); close(); trigger.value?.focus() }
function onKey(event: KeyboardEvent) {
  if (!props.options.length) return
  if (['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' ', 'Escape'].includes(event.key)) event.preventDefault()
  if (!expanded.value && ['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) { void open(); return }
  if (event.key === 'ArrowDown') active.value = (active.value + 1) % props.options.length
  if (event.key === 'ArrowUp') active.value = (active.value + props.options.length - 1) % props.options.length
  if (event.key === 'Home') active.value = 0
  if (event.key === 'End') active.value = props.options.length - 1
  if (event.key === 'Enter' || event.key === ' ') choose(active.value)
  if (event.key === 'Escape') { event.stopPropagation(); close(); trigger.value?.focus() }
  if (event.key === 'Tab') close()
  void nextTick(() => document.getElementById(`${id}-${active.value}`)?.scrollIntoView?.({ block: 'nearest' }))
}
onBeforeUnmount(close)
</script>

<template>
  <button ref="trigger" type="button" :class="['ui-select', { compact, expanded }]" :aria-label="label" aria-haspopup="listbox" :aria-expanded="expanded" :aria-controls="expanded ? id : undefined" :disabled="disabled" @click="expanded ? close() : open()" @keydown="onKey"><span>{{ selected }}</span><ChevronDown :size="14" /></button>
  <Teleport to="body">
    <div v-if="expanded" :id="id" ref="popup" class="ui-select-menu" role="listbox" :aria-label="label" tabindex="-1" :aria-activedescendant="`${id}-${active}`" :style="position" @keydown="onKey">
      <div v-for="(option, index) in options" :id="`${id}-${index}`" :key="option.value" :class="['ui-select-option', { selected: modelValue === option.value, focused: active === index }]" role="option" :aria-selected="modelValue === option.value" @pointermove="active = index" @mousedown.prevent @click="choose(index)"><span>{{ option.label }}</span><Check v-if="modelValue === option.value" :size="14" /></div>
    </div>
  </Teleport>
</template>
