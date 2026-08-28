import { defineStore } from 'valtio-define'

/** 设置模块：语言偏好由 i18n 统一管理。 */
export const setting = defineStore({
  state: () => ({
    language: null as string | null,
  }),
})
