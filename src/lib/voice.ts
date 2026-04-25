export interface VoiceRecognition {
  supported: boolean;
  start: () => void;
  stop: () => void;
  onResult: (cb: (transcript: string, isFinal: boolean) => void) => void;
  onEnd: (cb: () => void) => void;
  onError: (cb: (err: string) => void) => void;
}

export function createRecognition(): VoiceRecognition | null {
  const W = window as any;
  const Ctor = W.SpeechRecognition || W.webkitSpeechRecognition;
  if (!Ctor) return null;

  const rec = new Ctor();
  rec.continuous = false;
  rec.interimResults = true;
  rec.lang = "en-US";
  rec.maxAlternatives = 1;

  const callbacks = {
    result: (_t: string, _f: boolean) => {},
    end: () => {},
    error: (_e: string) => {},
  };

  rec.onresult = (e: any) => {
    let transcript = "";
    let isFinal = false;
    for (let i = e.resultIndex; i < e.results.length; i++) {
      transcript += e.results[i][0].transcript;
      if (e.results[i].isFinal) isFinal = true;
    }
    callbacks.result(transcript, isFinal);
  };
  rec.onend = () => callbacks.end();
  rec.onerror = (e: any) => callbacks.error(e.error || "error");

  return {
    supported: true,
    start: () => rec.start(),
    stop: () => rec.stop(),
    onResult: (cb) => (callbacks.result = cb),
    onEnd: (cb) => (callbacks.end = cb),
    onError: (cb) => (callbacks.error = cb),
  };
}

export function speak(text: string, opts: { voice?: string; rate?: number } = {}) {
  if (!("speechSynthesis" in window)) return;
  window.speechSynthesis.cancel();
  const utter = new SpeechSynthesisUtterance(text);
  utter.rate = opts.rate ?? 1;
  if (opts.voice) {
    const voices = window.speechSynthesis.getVoices();
    const match = voices.find((v) => v.name === opts.voice);
    if (match) utter.voice = match;
  }
  window.speechSynthesis.speak(utter);
}

export function stopSpeaking() {
  if ("speechSynthesis" in window) window.speechSynthesis.cancel();
}

export function ttsSupported() {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}
