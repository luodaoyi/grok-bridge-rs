/** @type {Record<string, string>} */
export default {
  "doc.title": "เซสชัน Grok Bridge",
  "doc.description":
    "คอนโซลเซสชันท้องถิ่นของ Grok Bridge: จัดการ Codex ซูเปอร์ไวเซอร์และ Grok ซับเอเจนต์",

  "app.skipToSessions": "ข้ามไปยังรายการเซสชัน",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "คอนโซลซูเปอร์ไวเซอร์และซับเอเจนต์",
  "app.subtitle":
    "แต่ละบทสนทนา Codex คือซูเปอร์ไวเซอร์ เซสชัน Grok ภายใต้เป็นซับเอเจนต์ถาวรที่พับได้ เทอร์มินัลรับเอาต์พุตแบบอ่านอย่างเดียวผ่าน WebSocket แบบเรียลไทม์ การปิดหน้าต่างนี้ไม่กระทบเซสชันที่ Runtime ถืออยู่",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "เปิด grok-bridge-rs บน GitHub",
  "app.github": "GitHub",

  "connection.initial": "กำลังเชื่อมต่อช่องถ่ายทอดสด",
  "connection.connected": "เชื่อมต่อช่องถ่ายทอดสดแล้ว",
  "connection.disconnected": "ช่องถ่ายทอดสดขาดการเชื่อมต่อ",
  "connection.retrying": "กำลังเชื่อมต่อช่องถ่ายทอดสดใหม่",
  "connection.reconnect": "เชื่อมต่อใหม่",
  "connection.reconnectAria": "เชื่อมต่อช่องถ่ายทอดสดด้วยตนเอง",

  "header.expandAll": "ขยายทั้งหมด",
  "header.collapseAll": "ยุบทั้งหมด",

  "lang.label": "ภาษา",
  "lang.aria": "ภาษาของอินเทอร์เฟซ",

  "theme.aria": "ชุดสี",
  "theme.auto": "อัตโนมัติ",
  "theme.light": "สว่าง",
  "theme.dark": "มืด",
  "theme.title": "ชุดสี{label}",

  "stats.aria": "สถิติเซสชัน",
  "stats.owners": "ซูเปอร์ไวเซอร์ (Codex)",
  "stats.sessions": "ซับเอเจนต์ (Grok)",
  "stats.working": "กำลังทำงาน",
  "stats.waiting": "รออินพุต",
  "stats.done": "เสร็จ / ว่าง",

  "stream.initial": "กำลังเชื่อมต่อช่องถ่ายทอดสด…",
  "stream.retrying": "กำลังเชื่อมต่อช่องถ่ายทอดสดใหม่…",
  "stream.disconnected": "ช่องถ่ายทอดสดขาดการเชื่อมต่อ รอเชื่อมต่อใหม่…",
  "stream.updated": "อัปเดตสด: {time}",
  "stream.waitingData": "เชื่อมต่อช่องถ่ายทอดสดแล้ว รอข้อมูลเซสชัน…",
  "stream.pushMode": "พุช: WebSocket · /api/events",
  "stream.error": "ข้อผิดพลาดช่องถ่ายทอดสด: {detail} (จะลองใหม่อัตโนมัติ)",

  "connecting.title": "กำลังเชื่อมต่อช่องถ่ายทอดสด",
  "connecting.body":
    "กำลังสร้างการเชื่อมต่อ WebSocket ไปยัง Runtime ท้องถิ่น สแนปช็อตเซสชันแรกจะแสดงทันที",

  "empty.title": "ไม่มีเซสชัน Grok",
  "empty.body":
    "การเรียกซูเปอร์ไวเซอร์ Codex ใหม่จะปรากฏที่นี่โดยอัตโนมัติ แต่ละเซสชัน Grok แสดงเทอร์มินัลและวงจรชีวิตในฐานะซับเอเจนต์ถาวร",

  "board.aria": "กลุ่มซูเปอร์ไวเซอร์",

  "update.title": "มีอัปเดต: v{version}",
  "update.body":
    "Runtime ปัจจุบันคือ v{current} ดาวน์โหลดและแทนที่ไบนารีท้องถิ่นด้วยตนเอง แล้วรีสตาร์ทเพื่อใช้งาน",
  "update.openRelease": "เปิด Release ล่าสุด",
  "update.dismiss": "เตือนฉันทีหลัง",

  "error.renderTitle": "ข้อผิดพลาดในการเรนเดอร์หน้า",
  "error.reload": "โหลดใหม่",
  "error.unknown": "ข้อผิดพลาดที่ไม่ทราบสาเหตุ",
  "error.timeout": "คำขอหมดเวลา จะลองใหม่อัตโนมัติ",
  "error.brand": "GROK BRIDGE",

  "activity.working": "กำลังทำงาน",
  "activity.waiting": "รออินพุต",
  "activity.done": "เสร็จ",
  "activity.stopped": "ออกแล้ว",
  "activity.unknown": "ไม่ทราบ",

  "client.unmanaged": "ไม่ได้ติดตาม",
  "client.connected": "Codex ออนไลน์",
  "client.disconnected": "Codex ตัดการเชื่อมต่อ",
  "client.orphaned": "รอการล้างอัตโนมัติ",
  "client.closing": "กำลังล้าง",
  "client.unknown": "ไม่ทราบ",

  "lifecycle.unmanaged": "ไม่มีการจัดการ",
  "lifecycle.connected": "ซูเปอร์ไวเซอร์ออนไลน์",
  "lifecycle.disconnected": "ซูเปอร์ไวเซอร์ออฟไลน์",
  "lifecycle.orphaned": "นับถอยหลังการล้าง",
  "lifecycle.closing": "กำลังล้าง",
  "lifecycle.unknown": "ไม่ทราบ",

  "badge.phase": "เฟส PTY: {phase}",

  "group.supervisor": "ซูเปอร์ไวเซอร์ · Codex",
  "group.unowned": "บทสนทนา Codex ที่ไม่มีป้ายกำกับ",
  "group.subagentCount": "ซับเอเจนต์ {n} รายการ",
  "group.closeAll": "ปิด Grok ทั้งหมดของ Codex นี้",
  "group.closeAllAria":
    "ปิด Grok ซับเอเจนต์ทั้งหมดภายใต้ซูเปอร์ไวเซอร์ {owner}",
  "group.summary.working": "กำลังทำงาน {n}",
  "group.summary.waiting": "รออินพุต {n}",
  "group.summary.done": "เสร็จ/ว่าง {n}",
  "group.summary.sep": " · ",
  "group.summary.none": "ไม่มีสถานะ",
  "group.idPrefix": "id {id}",

  "session.subagent": "ซับเอเจนต์",
  "session.close": "ปิด Grok",
  "session.closeAria": "ปิดซับเอเจนต์ {id}",
  "session.waitingCollapsed": "รอ: {reason}",
  "session.waitingNote": "รอ Codex: {reason}",
  "session.meta.id": "รหัสเซสชัน",
  "session.meta.pid": "โปรเซส",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "อัปเดตล่าสุด",
  "session.meta.client": "การเชื่อมต่อ Codex",
  "session.meta.autoClose": "นับถอยหลังการล้างอัตโนมัติ",
  "session.meta.hook": "Hook ล่าสุด",
  "session.meta.tool": "เครื่องมือปัจจุบัน",
  "session.meta.cwd": "ไดเรกทอรีทำงาน",
  "session.terminalAria": "เทอร์มินัลของซับเอเจนต์ {title}",

  "session.lifecycle.disconnectedTitle": "ซูเปอร์ไวเซอร์ออฟไลน์ — ยังไม่ปิด",
  "session.lifecycle.disconnectedBody":
    "ขั้น Running หรือ Waiting จะไม่ถูกปิดอัตโนมัติ ระยะผ่อนผันเริ่มหลังถึงสถานะ Idle หรือสถานะปลายทางที่ปลอดภัยเท่านั้น",
  "session.lifecycle.orphanedTitle": "นับถอยหลังปิดอัตโนมัติ",
  "session.lifecycle.orphanedCountdown": "มีสิทธิ์ล้างใน {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "เลยกำหนดสิทธิ์ล้างแล้ว — รอ Runtime ล้าง",
  "session.lifecycle.orphanedAt":
    "กำหนดสิทธิ์ล้างท้องถิ่น {at} Runtime จะล้างไม่นานหลังจากนั้น",
  "session.lifecycle.orphanedNoDeadline":
    "สถานะกำพร้า Runtime ไม่ได้รายงานกำหนดสิทธิ์ล้าง",
  "session.lifecycle.closingTitle": "กำลังปิดเซสชัน",
  "session.lifecycle.closingBody":
    "เซสชันที่มีการจัดการนี้กำลังถูกปิดอยู่",
  "session.lifecycle.collapsedOrphaned": "มีสิทธิ์ล้างใน {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "รอ Runtime ล้าง",
  "session.lifecycle.collapsedOrphanedUnknown": "กำพร้า — รอปิดอัตโนมัติ",
  "session.lifecycle.collapsedClosing": "กำลังปิด",

  "terminal.header": "เทอร์มินัล · ถ่ายทอดสดอ่านอย่างเดียว",
  "terminal.headerInteractive": "เทอร์มินัล · โต้ตอบได้",
  "terminal.aria": "เทอร์มินัลของ {id}",
  "terminal.resizeAria": "ปรับความสูงเทอร์มินัล",
  "terminal.resizeTitle": "ลากเพื่อปรับความสูงเทอร์มินัล",
  "terminal.resizeHint": "ใช้ปุ่มลูกศรปรับขนาด กด Enter เพื่อรีเซ็ตเป็นความสูงเริ่มต้น",
  "terminal.resizeValue": "{height} พิกเซล",

  "interactive.label": "แป้นพิมพ์",
  "interactive.on": "เปิด",
  "interactive.off": "ปิด",
  "interactive.aria": "สลับการป้อนแป้นพิมพ์แบบโต้ตอบสำหรับเทอร์มินัลทั้งหมด",
  "interactive.offHint": "เปิดการป้อนแป้นพิมพ์ไปยังเทอร์มินัล Grok ทั้งหมด",
  "interactive.warning": "โหมดโต้ตอบเปิดอยู่: การกดแป้นและการวางจะถูกส่งไปยังเซสชัน Grok ที่ทำงานอยู่ การปรับขนาดเทอร์มินัลจะตามวิวพอร์ตที่มองเห็นเสมอ การโหลดใหม่จะรีเซ็ตแป้นพิมพ์เป็นแบบอ่านอย่างเดียว",
  "interactive.disconnected": "ช่องถ่ายทอดสดขาดการเชื่อมต่อ อินพุตจะไม่ถูกส่งและไม่ถูกบัฟเฟอร์",
  "interactive.invalidPayload": "เพย์โหลดคำสั่งเทอร์มินัลไม่ถูกต้อง",
  "interactive.sendFailed": "ส่งคำสั่งเทอร์มินัลผ่านช่องถ่ายทอดสดไม่สำเร็จ",
  "interactive.error": "การป้อนเทอร์มินัลล้มเหลว: {detail}",
  "interactive.unavailable": "การป้อนเทอร์มินัลใช้ไม่ได้",
  "interactive.unavailableShort": "อินพุตออฟไลน์",

  "action.confirmCloseSession": "ปิด {id} และโปรเซส Grok ของมันหรือไม่?",
  "action.closedSession":
    "ปิดเซสชัน Grok {id} แล้ว ช่องถ่ายทอดสดจะพุชการอัปเดต",
  "action.closeFailed": "ปิดไม่สำเร็จ: {detail}",
  "action.confirmCloseGroup":
    "ปิดเซสชัน Grok ทั้ง {count} รายการภายใต้ Codex “{owner}” หรือไม่?",
  "action.groupEmpty": "กลุ่ม Codex นี้ไม่มีเซสชัน Grok ที่ใช้งานอยู่",
  "action.groupPartial":
    "ปิดแล้ว {closed}/{matched} เซสชัน ความล้มเหลว: {failures}",
  "action.groupClosed":
    "ปิดเซสชัน Grok ทั้ง {count} รายการภายใต้ Codex “{owner}” แล้ว ช่องถ่ายทอดสดจะพุชการอัปเดต",
  "action.failureJoin": ", ",
};
