<script setup lang="ts">
import {
  answerConfirm,
  canSubmitConfirm,
  confirmDialogState,
  setConfirmTyped,
} from "../lib/confirmDialog";

const state = confirmDialogState();

function onKey(e: KeyboardEvent) {
  if (!state.value.open) return;
  if (e.key === "Escape") {
    e.preventDefault();
    answerConfirm(false);
  } else if (e.key === "Enter") {
    if (state.value.requireTypedText && !canSubmitConfirm()) return;
    e.preventDefault();
    answerConfirm(true);
  }
}
</script>

<template>
  <div
    v-if="state.open"
    class="dialog-backdrop"
    role="presentation"
    @click.self="answerConfirm(false)"
    @keydown="onKey"
  >
    <div
      class="dialog"
      role="alertdialog"
      aria-modal="true"
      :aria-label="state.title"
      tabindex="-1"
    >
      <h3>{{ state.title }}</h3>
      <p style="margin: 0 0 8px; white-space: pre-wrap">{{ state.message }}</p>
      <div v-if="state.requireTypedText" style="margin-bottom: 10px">
        <label class="muted" style="display: block; margin-bottom: 4px">
          请输入：<code>{{ state.requireTypedText }}</code>
        </label>
        <input
          class="field"
          type="text"
          :value="state.typed"
          autocomplete="off"
          autocapitalize="none"
          spellcheck="false"
          placeholder="在此输入确认文字"
          @input="setConfirmTyped(($event.target as HTMLInputElement).value)"
        />
      </div>
      <div class="dialog-actions">
        <button type="button" class="btn" @click="answerConfirm(false)">
          {{ state.cancelText }}
        </button>
        <button
          type="button"
          class="btn"
          :class="state.danger ? 'danger' : 'primary'"
          :disabled="!!state.requireTypedText && state.typed !== state.requireTypedText"
          @click="answerConfirm(true)"
        >
          {{ state.confirmText }}
        </button>
      </div>
    </div>
  </div>
</template>
