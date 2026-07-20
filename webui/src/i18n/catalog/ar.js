/** @type {Record<string, string>} */
export default {
  "doc.title": "جلسات Grok Bridge",
  "doc.description":
    "وحدة تحكم الجلسات المحلية لـ Grok Bridge: إدارة مشرفي Codex ووكلاء Grok الفرعيين",

  "app.skipToSessions": "الانتقال إلى قائمة الجلسات",
  "app.brand": "GROK BRIDGE RUNTIME",
  "app.title": "وحدة تحكم المشرف والوكيل الفرعي",
  "app.subtitle":
    "كل محادثة Codex هي مشرف؛ جلسات Grok التابعة لها وكلاء فرعيون دائمون قابلون للطي. تستقبل الطرفية مخرجات للقراءة فقط عبر WebSocket في الوقت الفعلي؛ إغلاق هذه النافذة لا يؤثر على الجلسات التي يحتفظ بها Runtime.",
  "app.runtimeVersion": "Runtime v{version}",
  "app.githubTitle": "فتح grok-bridge-rs على GitHub",
  "app.github": "GitHub",

  "connection.initial": "جارٍ الاتصال بالقناة المباشرة",
  "connection.connected": "القناة المباشرة متصلة",
  "connection.disconnected": "انقطع الاتصال بالقناة المباشرة",
  "connection.retrying": "إعادة الاتصال بالقناة المباشرة",
  "connection.reconnect": "إعادة الاتصال",
  "connection.reconnectAria": "إعادة الاتصال يدويًا بالقناة المباشرة",

  "header.expandAll": "توسيع الكل",
  "header.collapseAll": "طي الكل",

  "lang.label": "اللغة",
  "lang.aria": "لغة الواجهة",

  "theme.aria": "سمة الألوان",
  "theme.auto": "تلقائي",
  "theme.light": "فاتح",
  "theme.dark": "داكن",
  "theme.title": "سمة {label}",

  "stats.aria": "إحصاءات الجلسات",
  "stats.owners": "المشرفون (Codex)",
  "stats.sessions": "الوكلاء الفرعيون (Grok)",
  "stats.working": "يعمل",
  "stats.waiting": "في الانتظار",
  "stats.done": "مكتمل / خامل",

  "stream.initial": "جارٍ الاتصال بالقناة المباشرة…",
  "stream.retrying": "إعادة الاتصال بالقناة المباشرة…",
  "stream.disconnected": "انقطع الاتصال بالقناة المباشرة، في انتظار إعادة الاتصال…",
  "stream.updated": "تحديث مباشر: {time}",
  "stream.waitingData": "القناة المباشرة متصلة، في انتظار بيانات الجلسات…",
  "stream.pushMode": "الدفع: WebSocket · /api/events",
  "stream.error": "خطأ في القناة المباشرة: {detail} (ستتم إعادة المحاولة تلقائيًا)",

  "connecting.title": "جارٍ الاتصال بالقناة المباشرة",
  "connecting.body":
    "يُجرى إنشاء اتصال WebSocket بـ Runtime المحلي؛ ستُعرض لقطة الجلسات الأولى فورًا.",

  "empty.title": "لا توجد جلسات Grok",
  "empty.body":
    "تظهر استدعاءات مشرف Codex الجديدة هنا تلقائيًا؛ تعرض كل جلسة Grok طرفيتها ودورة حياتها كوكيل فرعي دائم.",

  "board.aria": "مجموعات المشرفين",

  "update.title": "يتوفر تحديث: v{version}",
  "update.body":
    "إصدار Runtime الحالي هو v{current}. نزّل الثنائي المحلي واستبدله يدويًا، ثم أعد التشغيل لتطبيق التحديث.",
  "update.openRelease": "فتح أحدث Release",
  "update.dismiss": "ذكّرني لاحقًا",

  "error.renderTitle": "خطأ في عرض الصفحة",
  "error.reload": "إعادة التحميل",
  "error.unknown": "خطأ غير معروف",
  "error.timeout": "انتهت مهلة الطلب؛ تتم إعادة المحاولة تلقائيًا",
  "error.brand": "GROK BRIDGE",

  "activity.working": "يعمل",
  "activity.waiting": "في الانتظار",
  "activity.done": "مكتمل",
  "activity.stopped": "منتهٍ",
  "activity.unknown": "غير معروف",

  "client.unmanaged": "غير متتبَّع",
  "client.connected": "Codex متصل",
  "client.disconnected": "Codex غير متصل",
  "client.orphaned": "بانتظار التنظيف التلقائي",
  "client.closing": "جارٍ التنظيف",
  "client.unknown": "غير معروف",

  "lifecycle.unmanaged": "غير مُدار",
  "lifecycle.connected": "المشرف متصل",
  "lifecycle.disconnected": "المشرف غير متصل",
  "lifecycle.orphaned": "عدّاد التنظيف",
  "lifecycle.closing": "جارٍ التنظيف",
  "lifecycle.unknown": "غير معروف",

  "badge.phase": "مرحلة PTY: {phase}",

  "group.supervisor": "المشرف · Codex",
  "group.unowned": "محادثة Codex بلا تسمية",
  "group.subagentCount": "{n} وكلاء فرعيون",
  "group.closeAll": "إغلاق كل Grok لهذا Codex",
  "group.closeAllAria":
    "إغلاق جميع وكلاء Grok الفرعية تحت المشرف {owner}",
  "group.summary.working": "{n} يعمل",
  "group.summary.waiting": "{n} في الانتظار",
  "group.summary.done": "{n} مكتمل/خامل",
  "group.summary.sep": " · ",
  "group.summary.none": "لا توجد حالة",
  "group.idPrefix": "id {id}",

  "session.subagent": "وكيل فرعي",
  "session.close": "إغلاق Grok",
  "session.closeAria": "إغلاق الوكيل الفرعي {id}",
  "session.waitingCollapsed": "في الانتظار: {reason}",
  "session.waitingNote": "في انتظار Codex: {reason}",
  "session.meta.id": "معرّف الجلسة",
  "session.meta.pid": "العملية",
  "session.meta.pidValue": "PID {pid}",
  "session.meta.updated": "آخر تحديث",
  "session.meta.client": "اتصال Codex",
  "session.meta.autoClose": "عدّاد التنظيف التلقائي",
  "session.meta.hook": "أحدث Hook",
  "session.meta.tool": "الأداة الحالية",
  "session.meta.cwd": "دليل العمل",
  "session.terminalAria": "طرفية الوكيل الفرعي {title}",

  "session.lifecycle.disconnectedTitle": "المشرف غير متصل — الإغلاق لم يبدأ بعد",
  "session.lifecycle.disconnectedBody":
    "مراحل Running أو Waiting لا تُغلق تلقائيًا أبدًا. تبدأ فترة السماح فقط بعد مرحلة Idle آمنة أو مرحلة نهائية.",
  "session.lifecycle.orphanedTitle": "عدّاد الإغلاق التلقائي",
  "session.lifecycle.orphanedCountdown": "مؤهل للتنظيف خلال {remaining}",
  "session.lifecycle.orphanedCountdownDue":
    "تجاوز موعد الأهلية — في انتظار تنظيف Runtime",
  "session.lifecycle.orphanedAt":
    "موعد أهلية التنظيف المحلي {at}؛ ينظّف Runtime بعد ذلك بفترة قصيرة",
  "session.lifecycle.orphanedNoDeadline":
    "يتيم؛ لم يُبلغ Runtime عن موعد أهلية للتنظيف.",
  "session.lifecycle.closingTitle": "جارٍ إغلاق الجلسة",
  "session.lifecycle.closingBody":
    "تُغلق هذه الجلسة المُدارة الآن.",
  "session.lifecycle.collapsedOrphaned": "مؤهل للتنظيف خلال {remaining}",
  "session.lifecycle.collapsedOrphanedDue": "في انتظار تنظيف Runtime",
  "session.lifecycle.collapsedOrphanedUnknown": "يتيم — الإغلاق التلقائي معلّق",
  "session.lifecycle.collapsedClosing": "جارٍ الإغلاق الآن",

  "terminal.header": "الطرفية · قراءة فقط مباشرة",
  "terminal.headerInteractive": "الطرفية · تفاعلية",
  "terminal.aria": "طرفية {id}",
  "terminal.resizeAria": "تغيير ارتفاع الطرفية",
  "terminal.resizeTitle": "اسحب لتغيير ارتفاع الطرفية",
  "terminal.resizeHint": "استخدم مفاتيح الأسهم لتغيير الحجم؛ Enter يعيد الارتفاع الافتراضي",
  "terminal.resizeValue": "{height} بكسل",

  "interactive.label": "لوحة المفاتيح",
  "interactive.on": "تشغيل",
  "interactive.off": "إيقاف",
  "interactive.aria": "تبديل إدخال لوحة المفاتيح التفاعلي لجميع الطرفيات",
  "interactive.offHint": "تمكين إدخال لوحة المفاتيح لجميع طرفيات Grok",
  "interactive.warning": "الوضع التفاعلي مفعّل: تُرسل ضغطات المفاتيح واللصق إلى جلسات Grok المباشرة. تغيير حجم الطرفية يتبع دائمًا نافذة العرض المرئية. إعادة التحميل تعيد إدخال لوحة المفاتيح إلى وضع القراءة فقط.",
  "interactive.disconnected": "انقطع الاتصال بالقناة المباشرة؛ لا يُرسل الإدخال ولا يُخزَّن مؤقتًا.",
  "interactive.invalidPayload": "حمولة أمر الطرفية غير صالحة",
  "interactive.sendFailed": "فشل إرسال أمر الطرفية عبر القناة المباشرة",
  "interactive.error": "فشل إدخال الطرفية: {detail}",
  "interactive.unavailable": "إدخال الطرفية غير متاح",
  "interactive.unavailableShort": "الإدخال غير متصل",

  "action.confirmCloseSession": "إغلاق {id} وعملية Grok الخاصة به؟",
  "action.closedSession":
    "أُغلقت جلسة Grok {id}. ستدفع القناة المباشرة التحديثات.",
  "action.closeFailed": "فشل الإغلاق: {detail}",
  "action.confirmCloseGroup":
    "إغلاق جميع جلسات Grok البالغ عددها {count} تحت Codex «{owner}»؟",
  "action.groupEmpty": "لا توجد جلسات Grok نشطة في مجموعة Codex هذه.",
  "action.groupPartial":
    "أُغلق {closed}/{matched} من الجلسات؛ الإخفاقات: {failures}",
  "action.groupClosed":
    "أُغلقت جميع جلسات Grok البالغ عددها {count} تحت Codex «{owner}». ستدفع القناة المباشرة التحديثات.",
  "action.failureJoin": "، ",
};
