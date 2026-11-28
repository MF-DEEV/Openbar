// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

/** Docker containers and devcontainers (issue #330; sessions/containers.rs).
 * A container's sessions carry `container:folder:<path>` or
 * `container:name:<name>` where an SSH host's carry its alias. */

export const CONTAINER_PREFIX = "container:";

/** A picked container, as the prefs keep it. */
export interface ContainerTarget {
  /** A devcontainer's folder on this machine. */
  folder?: string;
  /** Any other container's name. */
  name?: string;
  /** The container Set up last wrote its hooks into, to tell a rebuilt one. */
  setUp?: string;
}

/** A running container, as sessions_containers_list gives it. */
export interface ContainerInfo {
  id: string;
  name: string;
  image: string;
  folder: string | null;
  user: string | null;
  workspace: string | null;
}

export function isContainer(alias: string): boolean {
  return alias.startsWith(CONTAINER_PREFIX);
}

export function targetId(target: ContainerTarget): string {
  return target.folder !== undefined ? `folder:${target.folder}` : `name:${target.name ?? ""}`;
}

export function targetAlias(target: ContainerTarget): string {
  return CONTAINER_PREFIX + targetId(target);
}

/** The devcontainer's project or the container's name, as a badge shows it. */
export function containerTitle(alias: string): string {
  const id = alias.startsWith(CONTAINER_PREFIX) ? alias.slice(CONTAINER_PREFIX.length) : alias;
  if (id.startsWith("folder:")) {
    const folder = id.slice("folder:".length);
    const last =
      folder
        .replace(/[\\/]+$/, "")
        .split(/[\\/]/)
        .pop() ?? "";
    return last || folder;
  }
  return id.startsWith("name:") ? id.slice("name:".length) : id;
}

export function targetOf(info: ContainerInfo): ContainerTarget {
  return info.folder ? { folder: info.folder } : { name: info.name };
}

/** What a badge or a group shows for a host key, SSH's or a container's. */
export function hostTitle(alias: string): string {
  return isContainer(alias) ? containerTitle(alias) : alias;
}
