#!/usr/bin/env python3
"""Offline advisory pipeline; synthetic data/review only, no LLM or deployment."""
import copy
import hashlib
import json
import pathlib
import subprocess
import tempfile
from backtest_smoke import generate, ROOT, START


def main():
    data, experiment = generate()
    with tempfile.TemporaryDirectory(prefix='trading-advisor-') as temp:
        root = pathlib.Path(temp)

        def save(name, value):
            (root / name).write_text(json.dumps(value))

        def run(*args, ok=True):
            result = subprocess.run(
                ['cargo', 'run', '--locked', '--quiet', '-p', 'trading-advisor', '--',
                 *[str(a) for a in args]], cwd=ROOT, capture_output=True, text=True)
            assert (result.returncode == 0) == ok, result.stderr
            return result

        context = {'as_of_ms': START - 1, 'symbol': 'BTC', 'observations': [
            {'source': 'SYNTHETIC_TEST_ONLY', 'available_at_ms': START - 2,
             'text': 'Fixture for advisory input; not model output or market evidence.'}]}
        save('context.json', context)
        save('dataset.json', data)
        save('experiment.json', experiment)
        run('prompt', root / 'context.json', root / 'prompt.json')
        prompt = json.loads((root / 'prompt.json').read_text())
        candidate = copy.deepcopy(experiment['candidates'][0])
        candidate['version'] = 'advisory-synthetic-v1'
        advice = {'context_hash': prompt['context_hash'], 'prompt_version': 'advisory-v1',
                  'model': 'fixture-not-a-model', 'regime': 'unknown',
                  'summary': 'Synthetic pipeline check only.', 'signal_explanation': '',
                  'post_trade_analysis': '', 'proposed_strategy': candidate}
        save('advice.json', advice)
        run('propose', root / 'context.json', root / 'advice.json', root / 'proposal.json')
        base = [root / 'proposal.json', root / 'dataset.json', root / 'experiment.json', '1']
        run('evaluate', *base, root / 'report.json')
        report = json.loads((root / 'report.json').read_text())
        assert report['eligible_for_manual_review'], report['blockers']
        assert report['report']['config']['risk'] == experiment['risk']
        assert report['report']['config']['candidates'] == [candidate]
        assert report['report']['source'] == 'SYNTHETIC_CLI_TEST_ONLY'
        # A fake review cannot authorize an export. The backtest is recomputed.
        save('approval.json', {'proposal_id': report['proposal_id'], 'evidence_hash': 'forged',
                               'reviewer': 'fixture', 'decision': 'approve_for_paper_review',
                               'reviewed_at_ms': START})
        run('export', *base, root / 'approval.json', root / 'strategy.json', ok=False)
        assert not (root / 'strategy.json').exists()
        # Synthetic operator receipt only, not a human approval or deployment.
        digest = hashlib.sha256(json.dumps(report, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
        save('fixture-approval.json', {'proposal_id': report['proposal_id'], 'evidence_hash': digest,
                                      'reviewer': 'TEST_FIXTURE_ONLY', 'decision': 'approve_for_paper_review',
                                      'reviewed_at_ms': START})
        run('export', *base, root / 'fixture-approval.json', root / 'strategy.json')
        assert json.loads((root / 'strategy.json').read_text()) == candidate
        changed = copy.deepcopy(experiment)
        changed['risk']['risk_fraction'] = '0.001'
        save('changed-experiment.json', changed)
        run('export', *base[:2], root / 'changed-experiment.json', '1',
            root / 'fixture-approval.json', root / 'changed-strategy.json', ok=False)
        assert not (root / 'changed-strategy.json').exists()
        # Raising the operator's sample threshold makes the same proposal ineligible.
        run('evaluate', *base[:-1], '1000', root / 'insufficient.json')
        assert not json.loads((root / 'insufficient.json').read_text())['eligible_for_manual_review']
        # Malformed model fields and mismatched context cannot be ingested.
        advice['approved'] = True
        save('bad-advice.json', advice)
        run('propose', root / 'context.json', root / 'bad-advice.json', root / 'bad.json', ok=False)
        original = (root / 'proposal.json').read_bytes()
        run('propose', root / 'context.json', root / 'advice.json', root / 'proposal.json', ok=False)
        assert original == (root / 'proposal.json').read_bytes()
        # An otherwise valid proposal using holdout context cannot be evaluated.
        context['as_of_ms'] = experiment['validation_start']
        save('future-context.json', context)
        run('prompt', root / 'future-context.json', root / 'future-prompt.json')
        advice.pop('approved')
        advice['context_hash'] = json.loads((root / 'future-prompt.json').read_text())['context_hash']
        save('future-advice.json', advice)
        run('propose', root / 'future-context.json', root / 'future-advice.json', root / 'future-proposal.json')
        run('evaluate', root / 'future-proposal.json', *base[1:], root / 'future-report.json', ok=False)
        print('PASS: advisory intake, actual backtest, fixed risk, holdout threshold, forged approval denial, immutable output; no model called')


if __name__ == '__main__':
    main()
