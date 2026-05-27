import { PriorityQueue } from './PriorityQueue';
import { describe, it } from 'vitest';

describe('TypedPriorityQueue', function () {
  let seed = 1;

  function random() {
    const x = Math.sin(seed++) * 10000;
    return x - Math.floor(x);
  }
  it('example1', function () {
    const x = new PriorityQueue(function (a, b) {
      return a < b;
    });
    x.add(1);
    x.add(0);
    x.add(5);
    x.add(4);
    x.add(3);
    if (x.poll() != 0) throw 'bug';
    if (x.poll() != 1) throw 'bug';
    if (x.poll() != 3) throw 'bug';
    if (x.poll() != 4) throw 'bug';
    if (x.poll() != 5) throw 'bug';
  });

  it('example2', function () {
    const x = new PriorityQueue(function (a, b) {
      return a > b;
    });
    x.add(1);
    x.add(0);
    x.add(5);
    x.add(4);
    x.add(3);
    if (x.poll() != 5) throw 'bug5';
    if (x.poll() != 4) throw 'bug4';
    if (x.poll() != 3) throw 'bug3';
    if (x.poll() != 1) throw 'bug1';
    if (x.poll() != 0) throw 'bug0';
  });

  it('Random', function () {
    for (let ti = 0; ti < 100; ti++) {
      const b = new PriorityQueue<number>(function (a, b) {
        return a < b;
      });
      const N = 1024 + ti;
      for (let i = 0; i < N; ++i) {
        b.add(Math.floor(random() * 1000000 + 1));
      }
      let v = 0;
      while (!b.isEmpty()) {
        const nv = b.poll();
        if (!nv || nv < v) throw 'bug';
        v = nv;
      }
    }
  });
  it('RandomArray', function () {
    for (let ti = 0; ti < 100; ti++) {
      const b = new PriorityQueue(function (a, b) {
        return a < b;
      });
      const array = [];
      const N = 1024 + ti;
      for (let i = 0; i < N; ++i) {
        const val = Math.floor(random() * 1000000 + 1);
        b.add(val);
        array.push(val);
      }
      array.sort(function (a, b) {
        return b - a;
      });
      while (!b.isEmpty()) {
        const nv = b.poll();
        const nx = array.pop();
        if (nv != nx) throw 'bug';
      }
    }
  });
  it('RandomArrayEnDe', function () {
    let val;
    for (let ti = 0; ti < 100; ti++) {
      const b = new PriorityQueue(function (a, b) {
        return a < b;
      });
      const array = [];
      const N = 16 + ti;
      for (let i = 0; i < N; ++i) {
        val = Math.floor(random() * 1000000 + 1);
        b.add(val);
        array.push(val);
      }
      array.sort(function (a, b) {
        return b - a;
      });
      for (let j = 0; j < 1000; ++j) {
        const nv = b.poll();
        const nx = array.pop();
        if (nv != nx) throw 'bug';
        val = Math.floor(random() * 1000000 + 1);
        b.add(val);
        array.push(val);
        array.sort(function (a, b) {
          return b - a;
        });
      }
    }
  });
});
