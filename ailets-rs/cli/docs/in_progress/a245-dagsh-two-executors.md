dagsh cli, It should be a refactory to the new approach. 

Before it was job-oriented. User would start a job and wait for its completion. 

Now we have a live core system which is similar to the Erlang shell or Smalltalk where user interface works directly with the world. We should adapt dagsh to the new point of view.

On the low level, technically, the ailetos should get its own Tokio runtime executor. If the command line interface needs asynchronous work, it should use its own executor. 

I think the concept of jobs should disappear. Instead, run is a run plus join on the final node. Instead of bg and fg which disappear, introduce "join" command which will wait till node is completed. Somehow it should be possible to interrupt waiting. 

We should have a command to print the output of an node to the console. Please design how to do it well, considering that output from several nodes can be mixed, and that we should not corrupt user input. 

