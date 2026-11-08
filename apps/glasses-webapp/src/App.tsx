import {useCallback, useEffect, useState} from 'react';
import {Theme} from '@astryxdesign/core/theme';
import {themes, defaultThemeSlug} from './astryx-themes';
import {
  AuthorizationCard,
  type ModalMode,
  type PendingDecision,
  type ProbeResults,
} from './components/AuthorizationCard';

function getInitialMode(): ModalMode {
  const params = new URLSearchParams(window.location.search);
  const modeParam = params.get('mode');
  if (
    modeParam === 'passkey' ||
    modeParam === 'failure' ||
    modeParam === 'start' ||
    modeParam === 'probe'
  ) {
    return modeParam;
  }
  return 'passkey';
}

function inspectWebAuthnSurface(): Pick<ProbeResults, 'navCreds' | 'pubKey'> {
  const hasNavCreds =
    typeof navigator !== 'undefined' && 'credentials' in navigator;
  const hasPubKey =
    typeof window !== 'undefined' && 'PublicKeyCredential' in window;
  return {
    navCreds: hasNavCreds ? typeof navigator.credentials : 'undefined',
    pubKey: hasPubKey
      ? typeof (window as unknown as {PublicKeyCredential: unknown})
          .PublicKeyCredential
      : 'undefined',
  };
}

function isBackendApiAvailable(): boolean {
  if (typeof window === 'undefined') return false;
  const win = window as unknown as {__WALKAWAY_API_ENABLED__?: boolean};
  if (win.__WALKAWAY_API_ENABLED__) return true;
  const {port, hostname} = window.location;
  return (
    port === '8799' ||
    port === '8443' ||
    hostname.endsWith('.trycloudflare.com')
  );
}

export default function App() {
  const [mode, setMode] = useState<ModalMode>(getInitialMode);
  const [probe, setProbe] = useState<ProbeResults>(() => ({
    ...inspectWebAuthnSurface(),
    createResult: 'NotSupportedError (Verified)',
    getResult: 'NotSupportedError (Verified)',
    reportCount: 0,
  }));
  const [pendingDecision, setPendingDecision] =
    useState<PendingDecision | null>(null);
  const [statusText, setStatusText] = useState<string>(
    'Swipe temple L/R to focus • Tap/Enter to select',
  );
  const [statusVariant, setStatusVariant] = useState<
    'neutral' | 'info' | 'success' | 'warning' | 'error'
  >('neutral');
  const [isSubmitting, setIsSubmitting] = useState<boolean>(false);

  const postReport = useCallback(
    async (action: string, payload: Record<string, unknown>) => {
      if (!isBackendApiAvailable()) return;
      try {
        const res = await fetch('/api/report', {
          method: 'POST',
          headers: {'Content-Type': 'application/json'},
          body: JSON.stringify({
            action,
            mode,
            userAgent: navigator.userAgent,
            viewport: `${window.innerWidth}x${window.innerHeight}`,
            isSecureContext: window.isSecureContext,
            ...payload,
          }),
        });
        if (res.ok) {
          setProbe(prev => ({...prev, reportCount: prev.reportCount + 1}));
        }
      } catch {
        // Offline / preview mode without backend server
      }
    },
    [mode],
  );

  // Initial capability report + fetch live report count & pending decisions
  useEffect(() => {
    const surface = inspectWebAuthnSurface();
    setProbe(prev => ({...prev, ...surface}));
    if (!isBackendApiAvailable()) return;
    void postReport('page_load_astryx', surface);

    let active = true;
    async function pollBackend() {
      if (!active || document.visibilityState === 'hidden') return;
      try {
        const [reportsRes, pendingRes] = await Promise.all([
          fetch('/api/reports'),
          fetch('/api/pending-decisions'),
        ]);
        if (!active) return;
        if (reportsRes.ok) {
          const data = (await reportsRes.json()) as {count?: number};
          if (typeof data.count === 'number') {
            setProbe(prev => ({...prev, reportCount: data.count ?? prev.reportCount}));
          }
        }
        if (pendingRes.ok) {
          const pData = (await pendingRes.json()) as {
            pending?: PendingDecision[];
          };
          const first = pData.pending?.[0] ?? null;
          setPendingDecision(first);
          if (first) {
            if (first.category === 'passkey_signin') setMode('passkey');
            else if (first.category === 'cascading_failure') setMode('failure');
            else if (first.category === 'ready_to_start') setMode('start');
          }
        }
      } catch {
        // Ignore when running against static vite preview in check-webapp.mjs
      }
    }

    void pollBackend();
    const intervalId = window.setInterval(pollBackend, 2000);
    const handleVisibility = () => {
      if (document.visibilityState === 'hidden') {
        window.clearInterval(intervalId);
      }
    };
    document.addEventListener('visibilitychange', handleVisibility);
    return () => {
      active = false;
      window.clearInterval(intervalId);
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  }, [postReport]);

  // 1D deterministic focus ring for Meta Ray-Ban Display temple swipes
  useEffect(() => {
    function getFocusableElements(): HTMLElement[] {
      return Array.from(
        document.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [role="radio"]:not([aria-disabled="true"]), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter(el => {
        const style = window.getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return (
          style.display !== 'none' &&
          style.visibility !== 'hidden' &&
          rect.width > 0 &&
          rect.height > 0
        );
      });
    }

    const initialList = getFocusableElements();
    const primaryBtn =
      initialList.find(el => el.tagName === 'BUTTON') ?? initialList[0];
    primaryBtn?.focus();

    function handleKeyDown(e: KeyboardEvent) {
      const list = getFocusableElements();
      if (list.length === 0) return;
      const currentIndex = list.indexOf(document.activeElement as HTMLElement);

      if (e.key === 'ArrowRight' || e.key === 'ArrowDown') {
        e.preventDefault();
        const nextIndex =
          currentIndex >= 0 ? (currentIndex + 1) % list.length : 0;
        list[nextIndex]?.focus();
      } else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') {
        e.preventDefault();
        const prevIndex =
          currentIndex > 0 ? currentIndex - 1 : list.length - 1;
        list[prevIndex]?.focus();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        if (
          !document.activeElement ||
          document.activeElement === document.body
        ) {
          list[0]?.focus();
        }
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const handleDecision = useCallback(
    async (approved: boolean) => {
      setIsSubmitting(true);
      const decisionId = pendingDecision?.decision_id ?? `demo_${mode}`;
      const actionLabel = approved
        ? mode === 'passkey'
          ? 'Approved Passkey ES256 Signature'
          : mode === 'failure'
            ? 'Initiated Manual Worktree Takeover'
            : 'Started Supervised Live Demo'
        : mode === 'passkey'
          ? 'Denied Passkey Request'
          : mode === 'failure'
            ? 'Aborted Failing Worktree Run'
            : 'Held Live Demo Session';

      if (isBackendApiAvailable()) {
        try {
          await fetch('/api/decision-ack', {
            method: 'POST',
            headers: {'Content-Type': 'application/json'},
            body: JSON.stringify({
              decision_id: decisionId,
              approved,
              mode,
              source: 'glasses_astryx_webapp',
            }),
          });
        } catch {
          // Continue in static preview mode
        }
      }

      await postReport('authorization_decision', {
        decision_id: decisionId,
        approved,
        actionLabel,
      });

      setStatusText(actionLabel);
      setStatusVariant(approved ? 'success' : 'error');
      setPendingDecision(null);
      setIsSubmitting(false);
    },
    [mode, pendingDecision, postReport],
  );

  const handleProbeCreate = useCallback(async () => {
    setIsSubmitting(true);
    setStatusText('Testing navigator.credentials.create()...');
    setStatusVariant('info');
    try {
      if (!navigator.credentials || !navigator.credentials.create) {
        throw new Error('navigator.credentials.create is undefined');
      }
      const challenge = new Uint8Array(32);
      crypto.getRandomValues(challenge);
      const userId = new Uint8Array(16);
      crypto.getRandomValues(userId);
      const cred = await navigator.credentials.create({
        publicKey: {
          challenge,
          rp: {name: 'Walkaway Glasses Probe', id: window.location.hostname},
          user: {id: userId, name: 'ryan@walkaway.dev', displayName: 'Ryan'},
          pubKeyCredParams: [
            {type: 'public-key', alg: -7},
            {type: 'public-key', alg: -257},
          ],
          timeout: 10000,
        },
      });
      const msg = `SUCCESS (${cred ? cred.id : 'null'})`;
      setProbe(prev => ({...prev, createResult: msg}));
      setStatusText(`create(): ${msg}`);
      setStatusVariant('success');
      await postReport('credentials_create_success', {result: msg});
    } catch (err) {
      const errorObj = err as {name?: string; message?: string};
      const msg = `${errorObj?.name || 'Error'}: ${errorObj?.message || String(err)}`;
      setProbe(prev => ({
        ...prev,
        createResult: `${errorObj?.name || 'Error'} (Thrown)`,
      }));
      setStatusText(msg);
      setStatusVariant('warning');
      await postReport('credentials_create_error', {error: msg});
    } finally {
      setIsSubmitting(false);
    }
  }, [postReport]);

  const handleProbeGet = useCallback(async () => {
    setIsSubmitting(true);
    setStatusText('Testing navigator.credentials.get()...');
    setStatusVariant('info');
    try {
      if (!navigator.credentials || !navigator.credentials.get) {
        throw new Error('navigator.credentials.get is undefined');
      }
      const challenge = new Uint8Array(32);
      crypto.getRandomValues(challenge);
      const assertion = await navigator.credentials.get({
        publicKey: {
          challenge,
          rpId: window.location.hostname,
          timeout: 10000,
        },
      });
      const msg = `SUCCESS (${assertion ? assertion.id : 'null'})`;
      setProbe(prev => ({...prev, getResult: msg}));
      setStatusText(`get(): ${msg}`);
      setStatusVariant('success');
      await postReport('credentials_get_success', {result: msg});
    } catch (err) {
      const errorObj = err as {name?: string; message?: string};
      const msg = `${errorObj?.name || 'Error'}: ${errorObj?.message || String(err)}`;
      setProbe(prev => ({
        ...prev,
        getResult: `${errorObj?.name || 'Error'} (Thrown)`,
      }));
      setStatusText(msg);
      setStatusVariant('warning');
      await postReport('credentials_get_error', {error: msg});
    } finally {
      setIsSubmitting(false);
    }
  }, [postReport]);

  return (
    <Theme theme={themes[defaultThemeSlug]} mode="dark">
      <AuthorizationCard
        mode={mode}
        onModeChange={nextMode => {
          setMode(nextMode);
          setStatusText(`Switched to ${nextMode.toUpperCase()} view`);
          setStatusVariant('neutral');
        }}
        probe={probe}
        pendingDecision={pendingDecision}
        statusText={statusText}
        statusVariant={statusVariant}
        isSubmitting={isSubmitting}
        onApprove={() => void handleDecision(true)}
        onReject={() => void handleDecision(false)}
        onProbeCreate={() => void handleProbeCreate()}
        onProbeGet={() => void handleProbeGet()}
      />
    </Theme>
  );
}
