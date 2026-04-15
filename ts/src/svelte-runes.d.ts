declare function $state<T>(initial: T): T;

declare namespace $state {
  function snapshot<T>(value: T): T;
}
