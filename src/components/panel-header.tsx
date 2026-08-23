import { Typography } from '@heroui/react'
import { cn } from 'tailwind-variants'

/**
 * 配置面板头部：标题 + 说明。
 * 与 config-plugin / core / profile 的「面板头」一致，
 * className 用于叠加 sticky / 背景等定位类（如 config-plugin 的 sticky 头部）。
 */
export interface PanelHeaderProps {
  title: string
  description: string
  className?: string
}

export function PanelHeader({ title, description, className }: PanelHeaderProps) {
  return (
    <div className={cn('space-y-2', className)}>
      <Typography type="h4">{title}</Typography>
      <Typography color="muted" type="body-sm">{description}</Typography>
    </div>
  )
}
