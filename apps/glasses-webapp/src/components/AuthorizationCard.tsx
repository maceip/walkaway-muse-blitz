import {Badge} from '@astryxdesign/core/Badge';
import {Button} from '@astryxdesign/core/Button';
import {Card} from '@astryxdesign/core/Card';
import {Heading} from '@astryxdesign/core/Heading';
import {HStack} from '@astryxdesign/core/HStack';
import {Icon} from '@astryxdesign/core/Icon';
import {
  MetadataList,
  MetadataListItem,
} from '@astryxdesign/core/MetadataList';
import {
  SegmentedControl,
  SegmentedControlItem,
} from '@astryxdesign/core/SegmentedControl';
import {StackItem} from '@astryxdesign/core/Stack';
import {StatusDot} from '@astryxdesign/core/StatusDot';
import {Text} from '@astryxdesign/core/Text';
import {VStack} from '@astryxdesign/core/VStack';

export type ModalMode = 'passkey' | 'failure' | 'start' | 'probe';

export interface ProbeResults {
  navCreds: string;
  pubKey: string;
  createResult: string;
  getResult: string;
  reportCount: number;
}

export interface PendingDecision {
  decision_id: string;
  category: string;
  title: string;
  summary: string;
}

export interface AuthorizationCardProps {
  mode: ModalMode;
  onModeChange: (mode: ModalMode) => void;
  probe: ProbeResults;
  pendingDecision: PendingDecision | null;
  statusText: string;
  statusVariant: 'neutral' | 'info' | 'success' | 'warning' | 'error';
  isSubmitting: boolean;
  onApprove: () => void;
  onReject: () => void;
  onProbeCreate: () => void;
  onProbeGet: () => void;
}

interface ModalCopy {
  badgeLabel: string;
  badgeVariant: 'info' | 'error' | 'success' | 'neutral';
  dotVariant: 'accent' | 'error' | 'success' | 'warning';
  heading: string;
  subtitle: string;
  metadata: Array<{label: string; value: string}>;
  primaryLabel: string;
  primaryIcon: 'check' | 'wrench';
  secondaryLabel: string;
  secondaryVariant: 'destructive' | 'secondary';
  secondaryIcon: 'close' | 'stop' | 'clock' | 'search';
}

export function AuthorizationCard({
  mode,
  onModeChange,
  probe,
  pendingDecision,
  statusText,
  statusVariant,
  isSubmitting,
  onApprove,
  onReject,
  onProbeCreate,
  onProbeGet,
}: AuthorizationCardProps) {
  const copyByMode: Record<ModalMode, ModalCopy> = {
    passkey: {
      badgeLabel: 'PASSKEY',
      badgeVariant: 'info',
      dotVariant: 'accent',
      heading: pendingDecision?.title || 'Sign in with Passkey?',
      subtitle:
        pendingDecision?.summary ||
        'Walkaway native browser intercepted a WebAuthn request.',
      metadata: [
        {label: 'Origin', value: '127.0.0.1:8797'},
        {label: 'RP ID', value: 'localhost'},
        {label: 'Method', value: 'ES256 Hardware Key'},
        {label: 'Account', value: 'demo_user'},
      ],
      primaryLabel: 'Approve Passkey',
      primaryIcon: 'check',
      secondaryLabel: 'Deny',
      secondaryVariant: 'destructive',
      secondaryIcon: 'close',
    },
    failure: {
      badgeLabel: 'ALERT',
      badgeVariant: 'error',
      dotVariant: 'error',
      heading: pendingDecision?.title || 'Cascading failure — Take over?',
      subtitle:
        pendingDecision?.summary ||
        '3 consecutive DOM action failures in active worktree.',
      metadata: [
        {label: 'Worktree', value: 'wt-slice3-auth'},
        {label: 'Failure', value: '3x sig mismatch'},
        {label: 'Stage', value: 'WebAuthn assert'},
        {label: 'Action', value: 'Manual takeover'},
      ],
      primaryLabel: 'Take Over',
      primaryIcon: 'wrench',
      secondaryLabel: 'Abort Run',
      secondaryVariant: 'destructive',
      secondaryIcon: 'stop',
    },
    start: {
      badgeLabel: 'READY',
      badgeVariant: 'success',
      dotVariant: 'success',
      heading: pendingDecision?.title || 'Ready to start the live demo?',
      subtitle:
        pendingDecision?.summary ||
        'All supervised browser fixtures and bridge channels are online.',
      metadata: [
        {label: 'Fixture', value: 'HN + Passkey'},
        {label: 'Bridge', value: 'UDP :8789 Ready'},
        {label: 'Browser', value: 'Walkaway Native'},
        {label: 'Mode', value: 'Supervised'},
      ],
      primaryLabel: 'Start Live Demo',
      primaryIcon: 'check',
      secondaryLabel: 'Hold',
      secondaryVariant: 'secondary',
      secondaryIcon: 'clock',
    },
    probe: {
      badgeLabel: 'PROBE',
      badgeVariant: 'neutral',
      dotVariant: 'warning',
      heading: 'window.credentials Status',
      subtitle:
        'Live WebAuthn probe on Meta Ray-Ban Display Chromium WebView.',
      metadata: [
        {label: 'Creds API', value: probe.navCreds},
        {label: 'PublicKey', value: probe.pubKey},
        {label: 'create()', value: probe.createResult},
        {label: 'get()', value: probe.getResult},
      ],
      primaryLabel: 'Test create()',
      primaryIcon: 'check',
      secondaryLabel: 'Test get()',
      secondaryVariant: 'secondary',
      secondaryIcon: 'search',
    },
  };

  const active = copyByMode[mode];

  return (
    <Card
      width={572}
      height={572}
      elevation="med"
      data-testid="primary-screen"
    >
      <VStack gap={3} height="100%" justify="between">
        {/* Top bar: Walkaway Glasses header + mode badge */}
        <HStack gap={2} vAlign="center" justify="between">
          <HStack gap={2} vAlign="center">
            <StatusDot
              variant={active.dotVariant}
              label={`System status: ${active.badgeLabel}`}
              isPulsing={pendingDecision !== null}
            />
            <Text type="label" weight="bold">
              Walkaway • Astryx HUD
            </Text>
          </HStack>
          <Badge label={active.badgeLabel} variant={active.badgeVariant} />
        </HStack>

        {/* Segmented mode selector (1D temple-swipe friendly) */}
        <SegmentedControl
          label="Authorization Screen Mode"
          value={mode}
          onChange={val => onModeChange(val as ModalMode)}
          layout="fill"
          size="sm"
        >
          <SegmentedControlItem value="passkey" label="Passkey" />
          <SegmentedControlItem value="failure" label="Failure" />
          <SegmentedControlItem value="start" label="Start" />
          <SegmentedControlItem value="probe" label="WebAuthn" />
        </SegmentedControl>

        {/* Main title & concise 2-line subtitle */}
        <VStack gap={1}>
          <Heading level={1} maxLines={2}>
            {active.heading}
          </Heading>
          <Text type="large" color="secondary" maxLines={2}>
            {active.subtitle}
          </Text>
        </VStack>

        {/* Elevated inner smoked-slate context card (#343944) */}
        <Card variant="muted" padding={3}>
          <MetadataList columns={2} label={{position: 'top'}}>
            {active.metadata.map(item => (
              <MetadataListItem key={item.label} label={item.label}>
                <Text type="body" weight="semibold" maxLines={1}>
                  {item.value}
                </Text>
              </MetadataListItem>
            ))}
          </MetadataList>
        </Card>

        {/* Single non-wrapping 1D horizontal action pill row */}
        <VStack gap={2}>
          <HStack gap={2} vAlign="center">
            <StackItem size="fill">
              <Button
                label={active.primaryLabel}
                variant="primary"
                size="lg"
                width="100%"
                isLoading={isSubmitting}
                icon={<Icon icon={active.primaryIcon} />}
                onClick={mode === 'probe' ? onProbeCreate : onApprove}
              />
            </StackItem>
            <StackItem size="fill">
              <Button
                label={active.secondaryLabel}
                variant={active.secondaryVariant}
                size="lg"
                width="100%"
                isLoading={isSubmitting}
                icon={<Icon icon={active.secondaryIcon} />}
                onClick={mode === 'probe' ? onProbeGet : onReject}
              />
            </StackItem>
          </HStack>

          {/* Footer telemetry / status pill bar */}
          <HStack gap={2} vAlign="center" justify="between">
            <StackItem size="fill">
              <Text type="supporting" maxLines={1}>
                {statusText}
              </Text>
            </StackItem>
            <Badge
              label={`Reports: ${probe.reportCount}`}
              variant={statusVariant}
            />
          </HStack>
        </VStack>
      </VStack>
    </Card>
  );
}
