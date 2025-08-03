 1 //These sectors are for 2048-byte sectors.
   //Multiply by 4 for devices with 512-byte sectors.
 3 if(cur_cmd.sector>=10000 && cur_cmd.sector<48000)
     tamperdetected=true;
 5
   //This is the legitimate read.
 7 cur_cmd.last_result = storage_read_sectors(
     IF_MD2(cur_cmd.lun,) cur_cmd.sector,
 9   MIN(READ_BUFFER_SIZE/SECTOR_SIZE, cur_cmd.count),
     cur_cmd.data[cur_cmd.data_select]
11 );

13 //Here, we wipe the buffer to demo antiforensics.
   if(tamperdetected){
15   for(i=0;i<READ_BUFFER_SIZE;i++)
       cur_cmd.data[cur_cmd.data_select][i]=0xFF;
17   //Clobber the buffer for testing.
     strcpy(cur_cmd.data[cur_cmd.data_select],
19          "Never gonna let you down.");

21   //Comment the following to make a harmless demo.
     //This writes the buffer back to the disk,
23   //eliminating any of the old contents.
     if(cur_cmd.sector>=48195)
25     storage_write_sectors(
            IF_MD2(cur_cmd.lun,)
27          cur_cmd.sector,
            MIN(WRITE_BUFFER_SIZE/SECTOR_SIZE, cur_cmd.count),
29          cur_cmd.data[cur_cmd.data_select]);
   }
