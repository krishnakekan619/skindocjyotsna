import { useEffect, useRef } from 'react';
import { api } from '../api';

const ACTIVITY_EVENTS = ['mousemove', 'mousedown', 'keydown', 'wheel', 'touchstart'] as const;

/**
 * Locks the screen after `minutes` without keyboard/mouse activity (DEC-024), and tells the
 * backend the user is active (at most once a minute) so its own idle check does not lock a user
 * who is busy on one screen. The backend enforces the lock independently of this hook.
 */
export function useIdleLock(minutes: number, onLock: () => void): void {
  const lastActivity = useRef(Date.now());
  const lastHeartbeat = useRef(Date.now());

  useEffect(() => {
    const markActive = () => {
      lastActivity.current = Date.now();
      if (Date.now() - lastHeartbeat.current > 60_000) {
        lastHeartbeat.current = Date.now();
        api.heartbeat().catch(() => undefined);
      }
    };
    ACTIVITY_EVENTS.forEach((e) => window.addEventListener(e, markActive, { passive: true }));
    const timer = window.setInterval(() => {
      if (Date.now() - lastActivity.current > minutes * 60_000) onLock();
    }, 15_000);
    return () => {
      ACTIVITY_EVENTS.forEach((e) => window.removeEventListener(e, markActive));
      window.clearInterval(timer);
    };
  }, [minutes, onLock]);
}
