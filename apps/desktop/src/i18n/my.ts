import type { Key } from './en';
// Burmese (Myanmar, Unicode — not Zawgyi). Missing keys fall back to English.
// TODO(i18n-review): needs native Burmese speaker review, especially long security explanations
// (unshare.body, devices.rotateHelp, invite.warning) and terms "vault" (သော့ခတ်သေတ္တာ) and "fingerprint" (လက်ဗွေ).
// Brand/product terms (Watchtower, Secret Key, Emergency Kit) intentionally kept in English.
export const my: Partial<Record<Key, string>> = {
  'nav.home': 'ပင်မ', 'nav.allItems': 'အရာအားလုံး', 'nav.favorites': 'အကြိုက်ဆုံးများ', 'nav.watchtower': 'Watchtower', 'nav.vaults': 'သော့ခတ်သေတ္တာများ', 'nav.tags': 'တဂ်များ',
  'nav.archive': 'မှတ်တမ်းသိမ်း', 'nav.deleted': 'မကြာသေးမီက ဖျက်ထားသည်', 'nav.newItem': 'အရာအသစ်', 'nav.search': 'ရှာဖွေရန်', 'nav.lock': 'သော့ခတ်ရန်', 'nav.settings': 'ဆက်တင်များ', 'nav.people': 'လူများ',
  'common.cancel': 'မလုပ်တော့ပါ', 'common.save': 'သိမ်းရန်', 'common.close': 'ပိတ်ရန်', 'common.copy': 'ကူးယူရန်', 'common.copied': 'ကူးယူပြီး', 'common.loading': 'ဖွင့်နေသည်…', 'common.error': 'အမှားဖြစ်သွားသည်- {msg}', 'common.saved': 'သိမ်းပြီး',
  'people.title': 'လူများ', 'people.tabMembers': 'အဖွဲ့ဝင်များ', 'people.tabInvitations': 'ဖိတ်ကြားချက်များ',
  'people.identity': 'သင့်အထောက်အထား', 'people.identityHelp': 'အခြားသူများက သင့်ကို ၎င်းတို့၏ သော့ခတ်သေတ္တာထဲ ထည့်နိုင်ရန် သင့် public key ကို မျှဝေပါ။ ၎င်းကို မျှဝေရန် လုံခြုံသည်။ ၎င်းတစ်ခုတည်းဖြင့် မည်သည့်အရာကိုမျှ ဖြည်၍မရပါ။',
  'people.publicKey': 'Public key', 'people.fingerprint': 'လက်ဗွေ', 'people.copyKey': 'Public key ကူးယူရန်', 'people.qr': 'သင့် public key ၏ QR ကုဒ်',
  'people.vaultMembers': 'သော့ခတ်သေတ္တာ အဖွဲ့ဝင်များ', 'people.you': 'သင်', 'people.unnamed': 'အမည်မဲ့ အဖွဲ့ဝင်', 'people.invite': 'ဖိတ်ရန် / မျှဝေရန်', 'people.remove': 'ဖယ်ရှားရန်',
  'people.noMembers': 'ဤသော့ခတ်သေတ္တာကို သင်သာ ဝင်ရောက်နိုင်သည်။', 'people.noVaults': 'သော့ခတ်သေတ္တာ မရှိသေးပါ။',
  'share.title': '“{vault}” ကို မျှဝေရန်', 'share.desc': 'လက်ခံသူ၏ public key ကို ကူးထည့်ပါ။ ၎င်းတို့သည် ဤသော့ခတ်သေတ္တာရှိ အရာအားလုံးကို ဖတ်ပြီး ပြင်ဆင်နိုင်ပါမည်။',
  'share.key': 'လက်ခံသူ၏ public key', 'share.label': 'အမည်တံဆိပ် (မထည့်လည်းရ)',
  'share.verify': 'အခြားနည်းလမ်းဖြင့် အတည်ပြုပါ', 'share.verifyHelp': 'ဤကုဒ်ကို ဖုန်းဖြင့် သို့မဟုတ် လူချင်းတွေ့၍ လက်ခံသူအား ဖတ်ပြပါ။ ၎င်းတို့၏ gitpass တွင် ပြသော ကုဒ်နှင့် တူရမည်။',
  'share.confirm': 'ဤ key ကို လက်ခံသူနှင့် သီးခြားနည်းလမ်းဖြင့် အတည်ပြုပြီးပါပြီ', 'share.submit': 'မျှဝေရန်', 'share.invalid': 'ဤအရာသည် public key နှင့် မတူပါ။',
  'share.done': '{name} နှင့် မျှဝေပြီးပါပြီ', 'share.pasteClipboard': 'ကူးထည့်ရန်',
  'unshare.title': '{name} ကို ဖယ်ရှားမလား?', 'unshare.body': '၎င်းတို့သည် နောက်ပိုင်း ပြောင်းလဲမှုများကို မရရှိတော့ပါ။ ချိန်ကိုက်ပြီးသား အချက်အလက်များမှာ ၎င်းတို့၏ စက်တွင် ကျန်နိုင်သောကြောင့် gitpass က သော့ကို အသစ်လဲပါမည်။ အရေးကြီးသော စကားဝှက်များကို ပြောင်းပါ။',
  'unshare.submit': 'ဖယ်ရှားရန်', 'unshare.done': 'အဖွဲ့ဝင်ကို ဖယ်ရှားပြီး သော့အသစ်လဲပြီးပါပြီ',
  'invite.title': 'မျှဝေထားသော သော့ခတ်သေတ္တာကို လက်ခံရန်', 'invite.desc': 'တစ်စုံတစ်ယောက် ပို့ထားသော မျှဝေဖိုင်ကို ရွေးပါ။', 'invite.warning': 'မျှဝေဖိုင်များတွင် ပို့သူကို အတည်ပြုချက် မပါပါ။ မည်သူမဆို အခြားသူဟု ဟန်ဆောင်၍ ဖန်တီးနိုင်သည်။ မျှော်လင့်ထားသော ဖိုင်ကိုသာ လက်ခံပြီး ပို့သူနှင့် အခြားနည်းလမ်းဖြင့် အတည်ပြုပါ။',
  'invite.path': 'မျှဝေဖိုင် လမ်းကြောင်း', 'invite.pick': 'ဖိုင်ရွေးရန်…', 'invite.accept': 'လက်ခံရန်', 'invite.done': '“{vault}” ကို ထည့်ပြီးပါပြီ', 'invite.empty': 'စောင့်ဆိုင်းနေသော ဖိတ်ကြားချက် မရှိပါ။',
  'settings.hibp': 'ပေါက်ကြားထားသော စကားဝှက်များနှင့် စစ်ဆေးရန်', 'settings.hibpHelp': 'မူလအားဖြင့် ပိတ်ထားသည်။ SHA-1 hash ၏ ပထမ စာလုံး ၅ လုံးကိုသာ Have I Been Pwned သို့ ပို့ပြီး ဤစက်ပေါ်တွင် တိုက်စစ်သည်။',
  'settings.title': 'ဆက်တင်များ', 'settings.general': 'အထွေထွေ', 'settings.security': 'လုံခြုံရေး', 'settings.devices': 'စက်များ', 'settings.sync': 'ချိန်ကိုက်ရန်', 'settings.about': 'အကြောင်း',
  'settings.theme': 'အပြင်အဆင်', 'settings.theme.dark': 'အမှောင်', 'settings.theme.light': 'အလင်း', 'settings.theme.system': 'စနစ်အတိုင်း', 'settings.language': 'ဘာသာစကား',
  'settings.autoLock': 'အလိုအလျောက် သော့ခတ်ချိန် (မိနစ်)', 'settings.clipboard': 'ကလစ်ဘုတ် ရှင်းချိန် (စက္ကန့်)', 'settings.biometric': 'ဇီဝမက်ထရစ်ဖြင့် ဖွင့်ရန်',
  'settings.biometricUnsupported': 'ဤစက်တွင် ဇီဝမက်ထရစ်ဖြင့် ဖွင့်ခြင်းကို မပံ့ပိုးသေးပါ။',
  'settings.changePassword': 'Master စကားဝှက် ပြောင်းရန်', 'settings.currentPassword': 'လက်ရှိ စကားဝှက်', 'settings.newPassword': 'စကားဝှက်အသစ်', 'settings.confirmPassword': 'စကားဝှက်အသစ်ကို အတည်ပြုပါ',
  'settings.passwordMismatch': 'စကားဝှက်များ မတူပါ', 'settings.passwordShort': 'အနည်းဆုံး စာလုံး ၁၀ လုံး သုံးပါ', 'settings.passwordChanged': 'Master စကားဝှက် ပြောင်းပြီးပါပြီ',
  'settings.rotateKey': 'Secret Key အသစ်လဲရန်', 'settings.rotateKeyConfirm': 'ယခု လဲရန်', 'settings.emergencyKit': 'သင့် Emergency Kit အသစ်', 'settings.kitWarning': 'ယခုပင် သိမ်းပါ သို့မဟုတ် ပုံနှိပ်ပါ။ နောက်တစ်ကြိမ် ပြမည်မဟုတ်ပါ။',
  'settings.kitSaved': 'Emergency Kit ကို သိမ်းပြီးပါပြီ', 'settings.done': 'ပြီးပါပြီ',
  'devices.current': 'ဤစက်', 'devices.revoked': 'ရုပ်သိမ်းပြီး', 'devices.lastSeen': 'နောက်ဆုံးတွေ့ {when}', 'devices.revoke': 'ရုပ်သိမ်းရန်', 'devices.revokeTitle': '“{name}” ကို ရုပ်သိမ်းမလား?',
  'devices.done': 'စက်ကို ရုပ်သိမ်းပြီးပါပြီ', 'devices.empty': 'စက် မတွေ့ပါ။',
  'sync.remote': 'Remote', 'sync.none': 'မချိတ်ဆက်ရသေးပါ', 'sync.lastSync': 'နောက်ဆုံး ချိန်ကိုက်ချိန်', 'sync.never': 'တစ်ခါမျှ မရှိ', 'sync.now': 'ချိန်ကိုက်ရန်', 'sync.merge': 'ပေါင်းစပ်၍ ချိန်ကိုက်ရန်', 'sync.result': 'ရလဒ်',
  'about.version': 'ဗားရှင်း', 'about.repo': 'ရင်းမြစ်ကုဒ်', 'about.issues': 'ပြဿနာ တင်ပြရန်',
  'palette.placeholder': 'အရာများ ရှာပါ သို့မဟုတ် command ရိုက်ပါ…', 'palette.commands': 'Command များ', 'palette.items': 'အရာများ', 'palette.empty': '“{q}” အတွက် ရလဒ် မရှိပါ',
  'cmd.newItem': 'အရာအသစ်', 'cmd.lock': 'gitpass ကို သော့ခတ်ရန်', 'cmd.settings': 'ဆက်တင်များသို့', 'cmd.people': 'လူများသို့', 'cmd.toggleTheme': 'အပြင်အဆင် ပြောင်းရန်', 'cmd.sync': 'ယခု ချိန်ကိုက်ရန်', 'cmd.shortcuts': 'ကီးဘုတ် ဖြတ်လမ်းများ', 'cmd.home': 'ပင်မသို့',
  'shortcuts.title': 'ကီးဘုတ် ဖြတ်လမ်းများ',
  'qa.placeholder': 'gitpass တွင် ရှာရန်', 'qa.copyPassword': 'စကားဝှက် ကူးယူရန်', 'qa.copyUsername': 'အသုံးပြုသူအမည် ကူးယူရန်', 'qa.passwordCopied': 'စကားဝှက် ကူးယူပြီး',
  'qa.usernameCopied': 'အသုံးပြုသူအမည် ကူးယူပြီး', 'qa.noResults': 'ကိုက်ညီသော အရာ မရှိပါ', 'qa.totp': 'တစ်ကြိမ်သုံး စကားဝှက်',
  // TODO(i18n-review): intentionally left to English fallback (unsure of precise Burmese security phrasing):
  // settings.rotateKeyHelp, devices.revokeBody, devices.rotate, devices.rotateHelp, sync.ahead/behind/dirty, about.license, shortcuts.*, palette.label, qa.label, qa.hint
};
