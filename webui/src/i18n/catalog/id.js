/** @type {Record<string, string>} */
export default {
  "doc.title": "Sesi Grok Bridge",
  "doc.description":
    "Konsol sesi lokal Grok Bridge: manajemen supervisor Codex dan subagen Grok",

  "app.skipToSessions": "Loncat ke daftar sesi",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Konsol Supervisor & Subagen",
  "app.subtitle":
    "Setiap percakapan Codex adalah supervisor; sesi Grok di bawahnya adalah subagen persisten yang dapat dilipat. Terminal menerima keluaran hanya-baca secara real time melalui WebSocket; menutup jendela ini tidak memengaruhi sesi yang dipegang Runtime.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Buka grok-bridge-rs di GitHub",
  "app.github": "GitHub",

  "connection.initial": "Menghubungkan saluran langsung",
  "connection.connected": "Saluran langsung terhubung",
  "connection.disconnected": "Saluran langsung terputus",
  "connection.retrying": "Menghubungkan ulang saluran langsung",
  "connection.reconnect": "Hubungkan ulang",
  "connection.reconnectAria": "Hubungkan ulang saluran langsung secara manual",

  "header.expandAll": "Bentangkan semua",
  "header.collapseAll": "Lipat semua",

  "lang.label": "Bahasa",
  "lang.aria": "Bahasa antarmuka",

  "theme.aria": "Tema warna",
  "theme.auto": "Otomatis",
  "theme.light": "Terang",
  "theme.dark": "Gelap",
  "theme.title": "Tema {label}",

  "stats.aria": "Statistik sesi",
  "stats.owners": "Supervisor (Codex)",
  "stats.sessions": "Subagen (Grok)",
  "stats.working": "Bekerja",
  "stats.waiting": "Menunggu",
  "stats.done": "Selesai / Menganggur",

  "stream.initial": "Menghubungkan saluran langsung…",
  "stream.retrying": "Menghubungkan ulang saluran langsung…",
  "stream.disconnected": "Saluran langsung terputus, menunggu koneksi ulang…",
  "stream.updated": "Pembaruan langsung: {time}",
  "stream.waitingData": "Saluran langsung terhubung, menunggu data sesi…",
  "stream.pushMode": "Push: WebSocket · /api/events",
  "stream.error": "Kesalahan saluran langsung: {detail} (akan mencoba ulang otomatis)",

  "connecting.title": "Menghubungkan saluran langsung",
  "connecting.body":
    "Koneksi WebSocket ke Runtime lokal sedang dibuat; snapshot sesi pertama akan ditampilkan segera.",

  "empty.title": "Tidak ada sesi Grok",
  "empty.body":
    "Panggilan supervisor Codex baru akan muncul di sini secara otomatis; setiap sesi Grok menampilkan terminal dan siklus hidupnya sebagai subagen persisten.",

  "board.aria": "Grup supervisor",

  "update.title": "Pembaruan tersedia: v{version}",
  "update.body":
    "Runtime saat ini adalah v{current}. Unduh dan ganti biner lokal secara manual, lalu mulai ulang untuk menerapkan.",
  "update.openRelease": "Buka Release terbaru",
  "update.dismiss": "Ingatkan nanti",

  "error.renderTitle": "Kesalahan perenderan halaman",
  "error.reload": "Muat ulang",
  "error.unknown": "kesalahan tidak diketahui",
  "error.timeout": "Permintaan habis waktu; mencoba ulang otomatis",
  "error.brand": "GROK BRIDGE",

  "activity.working": "Bekerja",
  "activity.waiting": "Menunggu",
  "activity.done": "Selesai",
  "activity.stopped": "Keluar",
  "activity.unknown": "Tidak diketahui",

  "client.unmanaged": "Tidak dilacak",
  "client.connected": "Codex daring",
  "client.disconnected": "Codex terputus",
  "client.orphaned": "Menunggu pembersihan otomatis",
  "client.closing": "Membersihkan",
  "client.unknown": "Tidak diketahui",

  "lifecycle.unmanaged": "Tidak dikelola",
  "lifecycle.connected": "Supervisor daring",
  "lifecycle.disconnected": "Supervisor luring",
  "lifecycle.orphaned": "Hitung mundur pembersihan",
  "lifecycle.closing": "Membersihkan",
  "lifecycle.unknown": "Tidak diketahui",

  "badge.phase": "Fase PTY: {phase}",

  "group.supervisor": "Supervisor · Codex",
  "group.unowned": "Percakapan Codex tanpa label",
  "group.subagentCount": "{n} subagen",
  "group.closeAll": "Tutup semua Grok untuk Codex ini",
  "group.closeAllAria":
    "Tutup semua subagen Grok di bawah supervisor {owner}",
  "group.summary.working": "{n} bekerja",
  "group.summary.waiting": "{n} menunggu",
  "group.summary.done": "{n} selesai/menganggur",
  "group.summary.sep": " · ",
  "group.summary.none": "Tidak ada status",
  "group.idPrefix": "id {id}",

  "session.subagent": "Subagen",
  "session.close": "Tutup Grok",
  "session.closeAria": "Tutup subagen {id}",
  "session.waitingCollapsed": "Menunggu: {reason}",
  "session.waitingNote": "Menunggu Codex: {reason}",
  "session.meta.id": "ID sesi",
  "session.meta.pid": "Proses",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Terakhir diperbarui",
  "session.meta.client": "Koneksi Codex",
  "session.meta.autoClose": "Hitung mundur pembersihan otomatis",
  "session.meta.hook": "Hook terbaru",
  "session.meta.tool": "Alat saat ini",
  "session.meta.cwd": "Direktori kerja",
  "session.terminalAria": "Terminal untuk subagen {title}",

  "session.lifecycle.disconnectedTitle": "Supervisor luring — belum ditutup",
  "session.lifecycle.disconnectedBody":
    "Tahap Running atau Waiting tidak pernah ditutup otomatis. Masa tenggang dimulai hanya setelah tahap Idle atau terminal yang aman.",
  "session.lifecycle.orphanedTitle": "Hitung mundur penutupan otomatis",
  "session.lifecycle.orphanedCountdown": "Memenuhi syarat pembersihan dalam {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Melewati batas kelayakan — menunggu pembersihan Runtime",
  "session.lifecycle.orphanedAt":
    "Batas kelayakan pembersihan lokal {at}; Runtime membersihkan segera setelahnya",
  "session.lifecycle.orphanedNoDeadline":
    "Yatim; Runtime tidak melaporkan batas kelayakan pembersihan.",
  "session.lifecycle.closingTitle": "Menutup sesi",
  "session.lifecycle.closingBody":
    "Sesi terkelola ini sedang ditutup sekarang.",
  "session.lifecycle.collapsedOrphaned": "Memenuhi syarat pembersihan dalam {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "Menunggu pembersihan Runtime",
  "session.lifecycle.collapsedOrphanedUnknown": "Yatim — penutupan otomatis tertunda",
  "session.lifecycle.collapsedClosing": "Sedang ditutup",

  "terminal.header": "Terminal · langsung hanya-baca",
  "terminal.headerInteractive": "Terminal · interaktif",
  "terminal.aria": "Terminal untuk {id}",
  "terminal.resizeAria": "Ubah tinggi terminal",
  "terminal.resizeTitle": "Seret untuk mengubah tinggi terminal",
  "terminal.resizeHint": "Gunakan tombol panah untuk mengubah ukuran; Enter mengatur ulang ke tinggi default",
  "terminal.resizeValue": "{height} piksel",

  "interactive.label": "Keyboard",
  "interactive.on": "Aktif",
  "interactive.off": "Nonaktif",
  "interactive.aria": "Alihkan input keyboard interaktif untuk semua terminal",
  "interactive.offHint": "Aktifkan input keyboard ke semua terminal Grok",
  "interactive.warning": "Mode interaktif aktif: ketukan tombol dan tempel dikirim ke sesi Grok langsung. Perubahan ukuran terminal selalu mengikuti viewport yang terlihat. Memuat ulang mengatur ulang input keyboard ke hanya-baca.",
  "interactive.disconnected": "Saluran langsung terputus; input tidak dikirim dan tidak di-buffer.",
  "interactive.invalidPayload": "Muatan perintah terminal tidak valid",
  "interactive.sendFailed": "Gagal mengirim perintah terminal melalui saluran langsung",
  "interactive.error": "Input terminal gagal: {detail}",
  "interactive.unavailable": "Input terminal tidak tersedia",
  "interactive.unavailableShort": "input luring",

  "action.confirmCloseSession": "Tutup {id} dan proses Grok-nya?",
  "action.closedSession":
    "Sesi Grok {id} ditutup. Saluran langsung akan mengirim pembaruan.",
  "action.closeFailed": "Penutupan gagal: {detail}",
  "action.confirmCloseGroup":
    "Tutup semua {count} sesi Grok di bawah Codex “{owner}”?",
  "action.groupEmpty": "Grup Codex ini tidak memiliki sesi Grok aktif.",
  "action.groupPartial":
    "Ditutup {closed}/{matched} sesi; kegagalan: {failures}",
  "action.groupClosed":
    "Semua {count} sesi Grok di bawah Codex “{owner}” ditutup. Saluran langsung akan mengirim pembaruan.",
  "action.failureJoin": ", ",
};
