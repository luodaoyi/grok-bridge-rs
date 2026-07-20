/** @type {Record<string, string>} */
export default {
  "doc.title": "Phiên Grok Bridge",
  "doc.description":
    "Bảng điều khiển phiên cục bộ Grok Bridge: quản lý giám sát viên Codex và agent con Grok",

  "app.skipToSessions": "Chuyển đến danh sách phiên",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "Bảng điều khiển Giám sát viên & Agent con",
  "app.subtitle":
    "Mỗi cuộc hội thoại Codex là một giám sát viên; các phiên Grok bên dưới là agent con bền vững có thể thu gọn. Terminal nhận đầu ra chỉ đọc theo thời gian thực qua WebSocket; đóng cửa sổ này không ảnh hưởng các phiên do Runtime giữ.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "Mở grok-bridge-rs trên GitHub",
  "app.github": "GitHub",

  "connection.initial": "Đang kết nối kênh trực tiếp",
  "connection.connected": "Kênh trực tiếp đã kết nối",
  "connection.disconnected": "Kênh trực tiếp đã ngắt",
  "connection.retrying": "Đang kết nối lại kênh trực tiếp",
  "connection.reconnect": "Kết nối lại",
  "connection.reconnectAria": "Kết nối lại thủ công kênh trực tiếp",

  "header.expandAll": "Mở rộng tất cả",
  "header.collapseAll": "Thu gọn tất cả",

  "lang.label": "Ngôn ngữ",
  "lang.aria": "Ngôn ngữ giao diện",

  "theme.aria": "Chủ đề màu",
  "theme.auto": "Tự động",
  "theme.light": "Sáng",
  "theme.dark": "Tối",
  "theme.title": "Chủ đề {label}",

  "stats.aria": "Thống kê phiên",
  "stats.owners": "Giám sát viên (Codex)",
  "stats.sessions": "Agent con (Grok)",
  "stats.working": "Đang làm việc",
  "stats.waiting": "Đang chờ",
  "stats.done": "Hoàn tất / Rảnh",

  "stream.initial": "Đang kết nối kênh trực tiếp…",
  "stream.retrying": "Đang kết nối lại kênh trực tiếp…",
  "stream.disconnected": "Kênh trực tiếp đã ngắt, đang chờ kết nối lại…",
  "stream.updated": "Cập nhật trực tiếp: {time}",
  "stream.waitingData": "Kênh trực tiếp đã kết nối, đang chờ dữ liệu phiên…",
  "stream.pushMode": "Đẩy: WebSocket · /api/events",
  "stream.error": "Lỗi kênh trực tiếp: {detail} (sẽ tự động thử lại)",

  "connecting.title": "Đang kết nối kênh trực tiếp",
  "connecting.body":
    "Đang thiết lập kết nối WebSocket tới Runtime cục bộ; ảnh chụp phiên đầu tiên sẽ được hiển thị ngay.",

  "empty.title": "Chưa có phiên Grok",
  "empty.body":
    "Các lần gọi giám sát viên Codex mới sẽ xuất hiện tự động tại đây; mỗi phiên Grok hiển thị terminal và vòng đời dưới dạng agent con bền vững.",

  "board.aria": "Nhóm giám sát viên",

  "update.title": "Có bản cập nhật: v{version}",
  "update.body":
    "Runtime hiện tại là v{current}. Hãy tải xuống và thay thế thủ công binary cục bộ, rồi khởi động lại để áp dụng.",
  "update.openRelease": "Mở Release mới nhất",
  "update.dismiss": "Nhắc tôi sau",

  "error.renderTitle": "Lỗi hiển thị trang",
  "error.reload": "Tải lại",
  "error.unknown": "lỗi không xác định",
  "error.timeout": "Yêu cầu hết thời gian; đang tự động thử lại",
  "error.brand": "GROK BRIDGE",

  "activity.working": "Đang làm việc",
  "activity.waiting": "Đang chờ",
  "activity.done": "Hoàn tất",
  "activity.stopped": "Đã thoát",
  "activity.unknown": "Không xác định",

  "client.unmanaged": "Chưa theo dõi",
  "client.connected": "Codex trực tuyến",
  "client.disconnected": "Codex đã ngắt",
  "client.orphaned": "Chờ dọn dẹp tự động",
  "client.closing": "Đang dọn dẹp",
  "client.unknown": "Không xác định",

  "lifecycle.unmanaged": "Chưa quản lý",
  "lifecycle.connected": "Giám sát viên trực tuyến",
  "lifecycle.disconnected": "Giám sát viên ngoại tuyến",
  "lifecycle.orphaned": "Đếm ngược dọn dẹp",
  "lifecycle.closing": "Đang dọn dẹp",
  "lifecycle.unknown": "Không xác định",

  "badge.phase": "Giai đoạn PTY: {phase}",

  "group.supervisor": "Giám sát viên · Codex",
  "group.unowned": "Cuộc hội thoại Codex chưa gắn nhãn",
  "group.subagentCount": "{n} agent con",
  "group.closeAll": "Đóng tất cả Grok của Codex này",
  "group.closeAllAria":
    "Đóng tất cả agent con Grok dưới giám sát viên {owner}",
  "group.summary.working": "{n} đang làm việc",
  "group.summary.waiting": "{n} đang chờ",
  "group.summary.done": "{n} hoàn tất/rảnh",
  "group.summary.sep": " · ",
  "group.summary.none": "Không có trạng thái",
  "group.idPrefix": "id {id}",

  "session.subagent": "Agent con",
  "session.close": "Đóng Grok",
  "session.closeAria": "Đóng agent con {id}",
  "session.waitingCollapsed": "Đang chờ: {reason}",
  "session.waitingNote": "Đang chờ Codex: {reason}",
  "session.meta.id": "ID phiên",
  "session.meta.pid": "Tiến trình",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "Cập nhật lần cuối",
  "session.meta.client": "Kết nối Codex",
  "session.meta.autoClose": "Đếm ngược dọn dẹp tự động",
  "session.meta.hook": "Hook gần nhất",
  "session.meta.tool": "Công cụ hiện tại",
  "session.meta.cwd": "Thư mục làm việc",
  "session.terminalAria": "Terminal của agent con {title}",

  "session.lifecycle.disconnectedTitle": "Giám sát viên ngoại tuyến — chưa đóng",
  "session.lifecycle.disconnectedBody":
    "Các giai đoạn Running hoặc Waiting không bao giờ bị tự đóng. Thời gian ân hạn chỉ bắt đầu sau khi vào Idle an toàn hoặc giai đoạn kết thúc.",
  "session.lifecycle.orphanedTitle": "Đếm ngược tự đóng",
  "session.lifecycle.orphanedCountdown": "Đủ điều kiện dọn dẹp sau {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "Đã quá hạn đủ điều kiện — đang chờ Runtime dọn dẹp",
  "session.lifecycle.orphanedAt":
    "Hạn đủ điều kiện dọn dẹp cục bộ {at}; Runtime dọn dẹp ngay sau đó",
  "session.lifecycle.orphanedNoDeadline":
    "Đã orphan; Runtime không báo hạn đủ điều kiện dọn dẹp.",
  "session.lifecycle.closingTitle": "Đang đóng phiên",
  "session.lifecycle.closingBody":
    "Phiên được quản lý này đang được đóng.",
  "session.lifecycle.collapsedOrphaned": "Đủ điều kiện dọn dẹp sau {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "Đang chờ Runtime dọn dẹp",
  "session.lifecycle.collapsedOrphanedUnknown": "Orphan — chờ tự đóng",
  "session.lifecycle.collapsedClosing": "Đang đóng",

  "terminal.header": "Terminal · chỉ đọc trực tiếp",
  "terminal.headerInteractive": "Terminal · tương tác",
  "terminal.aria": "Terminal của {id}",
  "terminal.resizeAria": "Thay đổi chiều cao terminal",
  "terminal.resizeTitle": "Kéo để thay đổi chiều cao terminal",
  "terminal.resizeHint": "Dùng phím mũi tên để đổi kích thước; Enter đặt lại chiều cao mặc định",
  "terminal.resizeValue": "{height} pixel",

  "interactive.label": "Bàn phím",
  "interactive.on": "Bật",
  "interactive.off": "Tắt",
  "interactive.aria": "Bật/tắt nhập bàn phím tương tác cho mọi terminal",
  "interactive.offHint": "Bật nhập bàn phím tới mọi terminal Grok",
  "interactive.warning": "Chế độ tương tác đang bật: phím bấm và dán được gửi tới các phiên Grok đang chạy. Kích thước terminal luôn theo viewport hiển thị. Tải lại trang sẽ đặt bàn phím về chỉ đọc.",
  "interactive.disconnected": "Kênh trực tiếp đã ngắt; đầu vào không được gửi và không được đệm.",
  "interactive.invalidPayload": "Payload lệnh terminal không hợp lệ",
  "interactive.sendFailed": "Không gửi được lệnh terminal qua kênh trực tiếp",
  "interactive.error": "Nhập terminal thất bại: {detail}",
  "interactive.unavailable": "Nhập terminal không khả dụng",
  "interactive.unavailableShort": "nhập ngoại tuyến",

  "action.confirmCloseSession": "Đóng {id} và tiến trình Grok của nó?",
  "action.closedSession":
    "Đã đóng phiên Grok {id}. Kênh trực tiếp sẽ đẩy cập nhật.",
  "action.closeFailed": "Đóng thất bại: {detail}",
  "action.confirmCloseGroup":
    "Đóng tất cả {count} phiên Grok dưới Codex “{owner}”?",
  "action.groupEmpty": "Nhóm Codex này không có phiên Grok đang hoạt động.",
  "action.groupPartial":
    "Đã đóng {closed}/{matched} phiên; lỗi: {failures}",
  "action.groupClosed":
    "Đã đóng tất cả {count} phiên Grok dưới Codex “{owner}”. Kênh trực tiếp sẽ đẩy cập nhật.",
  "action.failureJoin": ", ",
};
