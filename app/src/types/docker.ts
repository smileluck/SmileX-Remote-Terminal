/**
 * Docker 管理面板类型定义
 *
 * 字段对齐 `docker ps -a --format '{{json .}}'` / `docker images --format '{{json .}}'`
 * 的 per-line JSON 输出（见 services/docker.ts 的解析）。
 */

/** 容器条目 */
export interface DockerContainer {
  /** 容器 ID（短） */
  id: string
  /** 容器名 */
  name: string
  /** 关联镜像 */
  image: string
  /** 状态描述，如 "Up 2 hours" / "Exited (0) 3 days ago" */
  status: string
  /** 运行状态：running / exited / paused / created / ... */
  state: string
  /** 端口映射描述 */
  ports: string
  /** 创建时间描述 */
  createdAt: string
}

/** 镜像条目 */
export interface DockerImage {
  /** 仓库名 */
  repository: string
  /** 标签 */
  tag: string
  /** 镜像 ID（短） */
  id: string
  /** 大小描述 */
  size: string
  /** 创建时间描述 */
  createdSince: string
}

/** 从镜像启动容器的运行参数 */
export interface RunContainerOptions {
  /** 容器名（空 = 由 docker 自动生成） */
  name: string
  /** 端口映射（host:container） */
  ports: Array<{ host: string; container: string }>
  /** 环境变量（KEY=VALUE） */
  env: Array<{ key: string; value: string }>
  /** 后台运行（-d） */
  detach: boolean
  /** 重启策略（'' = 不设置） */
  restart: '' | 'always' | 'unless-stopped' | 'on-failure'
}

/** Docker 环境错误分类 */
export type DockerErrorKind =
  | 'not-installed'
  | 'no-permission'
  | 'daemon-down'
  | 'unknown'
