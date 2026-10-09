#!/usr/bin/env python3
"""Record the reference's CPU-busy and idle checks on scripted CPU loads.

The real `ClientController.Controller.SystemBusy`, `CurrentlyIdle` and
`GoodTimeToStartBackgroundWork` run on a stand-in controller (the real class,
not booted: a booted client calls `SystemBusy` from its own threads at
moments the script does not control). The cores' loads come from scripted
`/proc/stat` files read by the real psutil (`psutil.PROCFS_PATH`), so its
rounding to one decimal place, its iowait/guest accounting and its clamping
of decreasing counters all apply. Time is held still and moved by the script.

Each case: the options (percent, core count, idle on), a first `/proc/stat`
(the baseline psutil compares the next with), then steps of (milliseconds
after the baseline, a `/proc/stat`), with what each check answered.
"""
import collections, json, os, sys, tempfile, threading, types
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
os.environ.setdefault('QT_QPA_PLATFORM', 'offscreen')

NOW = [0]
FIELDS = ('user', 'nice', 'system', 'idle', 'iowait', 'irq', 'softirq', 'steal', 'guest', 'guest_nice')


def core(busy=0, idle=0, **other):
    """One core's counters: `busy` user jiffies and `idle` idle ones, plus any
    named field."""
    values = dict.fromkeys(FIELDS, 0)
    values['user'] = busy
    values['idle'] = idle
    values.update(other)
    return [values[f] for f in FIELDS]


def stat_text(cores):
    total = [sum(c[i] for c in cores) for i in range(len(FIELDS))]
    lines = ['cpu  ' + ' '.join(map(str, total))]
    lines += [f'cpu{i} ' + ' '.join(map(str, c)) for i, c in enumerate(cores)]
    return '\n'.join(lines) + '\nintr 0\nctxt 0\nbtime 0\n'


def advance(cores, deltas):
    return [[a + b for a, b in zip(c, d)] for c, d in zip(cores, deltas)]


def cases():
    """(name, percent, count, idle_on, baseline cores, [(ms, per-core deltas)])"""
    base1 = [core(1000, 1000)]
    base4 = [core(1000, 1000) for _ in range(4)]
    out = []
    # one core at 60%, the threshold moved across it
    for percent in (5, 59, 60, 61, 70, 99):
        out.append((f'60% at {percent}', percent, 1, True, base1, [(60001, [core(60, 40)])]))
    # psutil rounds to one decimal place before comparing
    for busy, idle in ((6004, 3996), (6005, 3995), (6006, 3994), (5996, 4004), (6049, 3951), (6050, 3950)):
        out.append((f'{busy}/{busy + idle} at 60', 60, 1, True, base1, [(60001, [core(busy, idle)])]))
    # iowait is idle; guest time is already in user time, so it leaves the total
    out.append(('iowait is not busy', 50, 1, True, base1, [(60001, [core(40, 0, iowait=60)])]))
    out.append(('system and irq are busy', 50, 1, True, base1, [(60001, [core(0, 40, system=30, irq=20, softirq=10)])]))
    out.append(('guest leaves the total', 50, 1, True, base1, [(60001, [core(50, 50, guest=20)])]))
    out.append(('steal is busy', 50, 1, True, base1, [(60001, [core(0, 49, steal=51)])]))
    # nothing ran: 0%
    out.append(('no time passed', 5, 1, True, base1, [(60001, [core(0, 0)])]))
    # a counter going backwards counts as no time
    out.append(('idle went backwards', 50, 1, True, base1, [(60001, [core(10, -500)])]))
    # how many cores must be over it
    loads = [core(90, 10), core(80, 20), core(10, 90), core(51, 49)]
    for count in (1, 2, 3, 4, None):
        out.append((f'three of four over 50, count {count}', 50, count, True, base4, [(60001, loads)]))
    # it looks again only once a minute has passed since it last looked
    out.append(('once a minute', 50, 1, True, base1, [
        (30000, [core(90, 10)]),
        (60000, [core(90, 10)]),
        (60001, [core(10, 90)]),
        (90000, [core(90, 10)]),
        (120001, [core(0, 100)]),
        (120002, [core(90, 10)]),
        (180002, [core(100, 0)]),
    ]))
    # the idle option: off, never idle, busy or not
    out.append(('idle off, quiet', 50, 1, False, base1, [(60001, [core(10, 90)])]))
    out.append(('idle off, busy', 50, 1, False, base1, [(60001, [core(90, 10)])]))
    out.append(('idle on, busy, cpu ignored', 50, None, True, base1, [(60001, [core(90, 10)])]))
    # busy, then the core count set to none ten seconds later: not busy at once
    out.append(('busy, then cpu ignored', 50, 1, True, base1, [
        (60001, [core(90, 10)]),
        (70001, [core(90, 10)], {'count': None}),
        (80001, [core(90, 10)], {'count': 1}),
        (120002, [core(90, 10)]),
    ]))
    # a new percent waits for the next look
    out.append(('percent raised between looks', 50, 1, True, base1, [
        (60001, [core(60, 40)]),
        (70001, [core(60, 40)], {'percent': 70}),
        (120002, [core(60, 40)]),
    ]))
    return out


def record():
    import psutil
    from hydrus.core import HydrusTime, HydrusGlobals as HG
    from hydrus.client import ClientController, ClientOptions, ClientDefaults, ClientGlobals as CG
    from hydrus.core import HydrusConstants as HC

    HydrusTime.time = types.SimpleNamespace(time=lambda: NOW[0] / 1000.0)

    class Stand(ClientController.Controller):
        def __init__(self):
            self._timestamps_lock = threading.Lock()
            self._timestamps_ms = collections.defaultdict(lambda: 0)
            self._sleep_lock = threading.Lock()
            self._just_woke_from_sleep = False
            self._system_busy = False
            self._program_is_shutting_down = False
            self._previously_idle = False
            self._idle_started = None
            self.new_options = ClientOptions.ClientOptions()
            self.options = ClientDefaults.GetClientDefaultOptions()

        def pub(self, *args, **kwargs):
            pass

    out = []
    stat_dir = tempfile.mkdtemp()
    psutil.PROCFS_PATH = stat_dir
    stat_path = os.path.join(stat_dir, 'stat')
    for (name, percent, count, idle_on, baseline, steps) in cases():
        c = Stand()
        CG.client_controller = c
        HG.controller = c
        HC.options = c.options
        c.new_options.SetInteger('system_busy_cpu_percent', percent)
        c.new_options.SetNoneableInteger('system_busy_cpu_count', count)
        c.options['idle_normal'] = idle_on
        for k in ('idle_period', 'idle_mouse_period'):
            c.options[k] = None
        c.new_options.SetNoneableInteger('idle_mode_client_api_timeout', None)
        # booted long enough ago to be allowed idle
        t0 = 10_000_000_000
        NOW[0] = t0 - 200_000
        c.TouchTime('boot')
        NOW[0] = t0
        with open(stat_path, 'w') as f:
            f.write(stat_text(baseline))
        psutil.cpu_percent(percpu=True)  # the baseline
        c.TouchTime('last_cpu_check')
        cores = baseline
        row = {'name': name, 'percent': percent, 'count': count, 'idle_on': idle_on,
               'baseline': stat_text(baseline), 'steps': []}
        for (ms, deltas, *change) in steps:
            change = change[0] if change else {}
            if 'count' in change:
                count = change['count']
                c.new_options.SetNoneableInteger('system_busy_cpu_count', count)
            if 'percent' in change:
                c.new_options.SetInteger('system_busy_cpu_percent', change['percent'])
            cores = advance(cores, deltas)
            text = stat_text(cores)
            with open(stat_path, 'w') as f:
                f.write(text)
            NOW[0] = t0 + ms
            c.TouchTime('last_sleep_check')  # it runs often; no sleep here
            looked = HydrusTime.TimeHasPassedMS(c.GetTimestampMS('last_cpu_check') + 60000)
            percents = None
            if looked and count is not None:
                # what psutil would say, without disturbing its baseline
                percents = [round(x, 1) for x in _percents(psutil, row, cores)]
            busy = c.SystemBusy()
            idle = c.CurrentlyIdle()
            good = c.GoodTimeToStartBackgroundWork()
            row['steps'].append({'ms': ms, 'change': change, 'stat': text, 'looked': looked, 'percents': percents,
                                 'busy': busy, 'idle': idle, 'good_time_for_background_work': good})
        out.append(row)
    return out




def _percents(psutil, row, cores):
    # the per-core percent psutil computes from the previous look to this
    # one, with its own formula (a copy of its baseline is used)
    import copy
    tid = threading.current_thread().ident
    saved = copy.copy(psutil._last_per_cpu_times.get(tid))
    try:
        return psutil.cpu_percent(percpu=True)
    finally:
        psutil._last_per_cpu_times[tid] = saved


def main():
    result = record()
    with open(os.path.join(HERE, 'fixtures', 'cpu_busy.json'), 'w') as f:
        json.dump(result, f, indent=1, ensure_ascii=False)
        f.write('\n')
    print('recorded', len(result), 'cpu busy cases')


if __name__ == '__main__':
    main()
