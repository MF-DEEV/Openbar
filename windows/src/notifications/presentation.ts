// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import discordIcon from "./icons/discord-brand.svg";
import messengerIcon from "./icons/messenger.svg";
import type { Notice } from "../island/store";
import type { Item } from "./types";

/** Match the source identity, never words in a message or a sender's name. */
export function notificationSource(item: Pick<Item, "app" | "aumid">) {
  const name = item.app.trim();
  const id = item.aumid.toLowerCase();
  if (isDiscordNotification(item)) {
    return { name: name || "Discord", fallbackIcon: discordIcon };
  }
  if (/^(?:facebook\s+)?messenger$/i.test(name) || /^facebook\.317180b0bb486_[a-z0-9]+!/.test(id)) {
    return { name: "Messenger", fallbackIcon: messengerIcon };
  }
  return { name: name || "Notification", fallbackIcon: undefined };
}

export function isDiscordNotification(item: Pick<Item, "app" | "aumid">): boolean {
  const name = item.app.trim();
  const id = item.aumid.toLowerCase();
  return (
    /^discord(?:\s+(?:canary|ptb|development))?$/i.test(name) ||
    /^com\.squirrel\.discord(?:canary|ptb|development)?\.discord(?:canary|ptb|development)?$/.test(id)
  );
}

export function notificationPresentation(item: Item) {
  const source = notificationSource(item);
  const discord = isDiscordNotification(item);
  const title = item.title.trim();
  // The first Windows toast text carries the sender for messaging apps. Some
  // privacy settings replace it with the app name; don't invent a sender then.
  const sender = title && title.toLocaleLowerCase() !== source.name.toLocaleLowerCase() ? title : undefined;
  return {
    app: source.name,
    sender,
    message: item.body?.trim() || undefined,
    icon: discord ? source.fallbackIcon : item.icon || source.fallbackIcon,
    fallbackIcon: source.fallbackIcon,
    image: discord ? discordImageUrl(item.body || "") : undefined,
  };
}

/** Only display image URLs explicitly present in the Discord toast text. */
export function discordImageUrl(body: string): string | undefined {
  for (const match of body.matchAll(/https?:\/\/[^\s<>"']+/gi)) {
    const candidate = match[0].replace(/[),.!?;]+$/, "");
    try {
      const url = new URL(candidate);
      if (
        url.protocol === "https:" &&
        ["cdn.discordapp.com", "media.discordapp.net"].includes(url.hostname) &&
        url.pathname.startsWith("/attachments/") &&
        /\.(?:png|jpe?g|gif|webp|avif)$/i.test(url.pathname)
      ) return url.href;
    } catch { /* A malformed link is ordinary notification text. */ }
  }
  return undefined;
}

export function notificationNotice(item: Item): Notice {
  const display = notificationPresentation(item);
  return {
    id: `notification:${item.id}`,
    kind: "systemNotification",
    title: display.sender ? `${display.app} · ${display.sender}` : display.app,
    priority: 1,
    duration: 5,
    opens: "notifications",
    image: display.icon,
    imageFallback: display.fallbackIcon,
    notification: {
      id: item.id,
      app: display.app,
      sender: display.sender,
      body: display.message,
      image: display.image,
      aumid: item.aumid,
      time: item.time,
    },
  };
}
